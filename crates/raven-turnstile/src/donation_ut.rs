// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Unit tests for the IPC priority donation layer.

use super::*;
use alloc::vec;

fn prio(level: u8) -> Priority {
    Priority::new(level)
}

fn next_random(state: &mut u64) -> u64 {
    let mut s = *state;
    s ^= s << 13;
    s ^= s >> 7;
    s ^= s << 17;
    *state = s;
    s
}

#[test]
fn call_donates_priority_to_server() {
    let mut d = Donation::new();
    d.register(0, prio(10)).unwrap(); // server
    d.register(1, prio(2)).unwrap(); // client (higher)
    d.begin_call(1, 0).unwrap();
    assert_eq!(d.effective_priority(0), Some(prio(2)));
    assert_eq!(d.effective_priority(1), Some(prio(2))); // client keeps its own priority
    d.check_invariants().expect("invariant");
}

#[test]
fn call_and_lock_donation_flow_transitively() {
    // C (high) calls S; S waits on O's lock; O inherits C's priority through both edges.
    let mut d = Donation::new();
    d.register(0, prio(20)).unwrap(); // O, low
    d.register(1, prio(10)).unwrap(); // S
    d.register(2, prio(0)).unwrap(); // C, high
    d.acquire(0, 0).unwrap();
    d.begin_call(2, 1).unwrap();
    assert_eq!(d.effective_priority(1), Some(prio(0)));
    d.block(0, 1).unwrap();
    assert_eq!(d.effective_priority(0), Some(prio(0)));
    d.check_invariants().expect("invariant");
}

#[test]
fn inversion_case_owner_runs_at_clients_priority_then_restores() {
    let mut d = Donation::new();
    d.register(0, prio(20)).unwrap(); // low-priority lock owner O
    d.register(1, prio(10)).unwrap(); // server S
    d.register(2, prio(0)).unwrap(); // high-priority client C
    d.acquire(0, 0).unwrap();

    d.begin_call(2, 1).unwrap();
    d.block(0, 1).unwrap(); // S waits on O's lock
    // No inversion: the low-priority owner runs at the client's priority.
    assert_eq!(d.effective_priority(0), Some(prio(0)));

    // O releases the lock (hands it to S).
    assert_eq!(d.wake_one(0), Some(1));
    assert_eq!(d.effective_priority(0), Some(prio(20)));

    // The call completes; the server returns to its base priority.
    d.end_call(2, 1).unwrap();
    assert_eq!(d.effective_priority(1), Some(prio(10)));
    assert_eq!(d.effective_priority(0), Some(prio(20)));
    d.check_invariants().expect("invariant");
}

#[test]
fn call_edges_are_managed_exactly() {
    let mut d = Donation::new();
    d.register(0, prio(10)).unwrap();
    d.register(1, prio(10)).unwrap();
    d.register(2, prio(10)).unwrap();

    assert_eq!(d.begin_call(0, 1), Ok(()));
    assert_eq!(d.call_edges(), vec![(0, 1)]);
    // Duplicate (client already calling).
    assert_eq!(d.begin_call(0, 2), Err(DonationError::DuplicateCall));
    // Unknown thread.
    assert_eq!(d.begin_call(99, 1), Err(DonationError::UnknownThread));
    // end_call with a mismatched server is stale.
    assert_eq!(d.end_call(0, 2), Err(DonationError::StaleCall));
    // cancel removes the call.
    assert_eq!(d.cancel_call(0), Ok(()));
    assert_eq!(d.cancel_call(0), Err(DonationError::StaleCall));
    assert!(d.call_edges().is_empty());
}

#[test]
fn client_and_server_death_remove_right_edges() {
    let mut d = Donation::new();
    for i in 0..6 {
        d.register(i as ThreadId, prio(10)).unwrap();
    }
    d.begin_call(0, 1).unwrap(); // client 0 -> server 1
    d.begin_call(2, 1).unwrap(); // client 2 -> server 1
    d.begin_call(3, 4).unwrap(); // client 3 -> server 4

    // Client death removes exactly that client's edge.
    assert_eq!(d.client_death(0), 1);
    assert_eq!(d.call_edges(), vec![(2, 1), (3, 4)]);

    // Server death removes every call to it.
    assert_eq!(d.server_death(1), 1);
    assert_eq!(d.call_edges(), vec![(3, 4)]);

    // Idempotent.
    assert_eq!(d.client_death(0), 0);
    assert_eq!(d.server_death(1), 0);
    d.check_invariants().expect("invariant");
}

#[test]
fn server_inherits_max_over_multiple_clients() {
    let mut d = Donation::new();
    d.register(0, prio(30)).unwrap(); // server
    d.register(1, prio(10)).unwrap(); // client (lower number = higher priority)
    d.register(2, prio(20)).unwrap(); // client
    d.begin_call(1, 0).unwrap();
    d.begin_call(2, 0).unwrap();
    // Server inherits the most urgent client (10).
    assert_eq!(d.effective_priority(0), Some(prio(10)));
}

#[test]
fn donation_model_million_ops() {
    const SERVERS: u64 = 3;
    const CLIENTS: u64 = 64;
    const LOCKS: u64 = 16;
    const OPS: u64 = 1_000_000;
    const TOTAL: u64 = SERVERS + CLIENTS;

    let mut d = Donation::new();
    for i in 0..TOTAL {
        d.register(i as ThreadId, prio((i % 7) as u8)).unwrap();
    }

    let mut rng = 0x2545_f491_4f6c_dd1du64;
    for step in 0..OPS {
        let roll = next_random(&mut rng) % 100;
        let client = (next_random(&mut rng) % CLIENTS + SERVERS) as ThreadId;
        let server = (next_random(&mut rng) % SERVERS) as ThreadId;
        let lock = (next_random(&mut rng) % LOCKS) as ObjectId;
        let thread = (next_random(&mut rng) % TOTAL) as ThreadId;

        match roll {
            0..=19 => {
                let _ = d.begin_call(client, server);
            }
            20..=29 => {
                let _ = d.end_call(client, server);
            }
            30..=34 => {
                let _ = d.cancel_call(client);
            }
            35..=37 => {
                let _ = d.client_death(client);
            }
            38..=39 => {
                let _ = d.server_death(server);
            }
            40..=54 => {
                let _ = d.acquire(lock, thread);
            }
            55..=69 => {
                let _ = d.block(lock, thread);
            }
            70..=79 => {
                let _ = d.wake_one(lock);
            }
            80..=89 => {
                let _ = d.wake_all(lock);
            }
            _ => {
                let _ = d.cancel(lock, thread);
            }
        }

        if step.is_multiple_of(512) {
            d.check_invariants().expect("donation invariant violation");
        }
    }
    d.check_invariants()
        .expect("final donation invariant violation");
    assert!(
        d.max_chain_depth() <= TOTAL as usize,
        "depth bound exceeded: {}",
        d.max_chain_depth()
    );
}

#[cfg(feature = "lockstat")]
#[test]
fn stats_and_objects_are_delegated() {
    let mut d = Donation::new();
    d.register(0, prio(10)).unwrap(); // owner
    d.register(1, prio(2)).unwrap(); // waiter
    d.acquire(7, 0).unwrap();
    d.block(7, 1).unwrap();

    let stats = d.stats(7);
    assert_eq!(stats.wait_entries, 1);
    assert_eq!(stats.max_queue_depth, 1);
    assert!(stats.inherited_priority_changes >= 1);
    assert_eq!(stats.timeouts, 0);

    let objects: vec::Vec<ObjectId> = d.objects().collect();
    assert_eq!(objects, vec![7]);
    // An untouched object reports the default (all-zero) snapshot.
    assert_eq!(d.stats(99), crate::LockStats::default());
}

#[cfg(feature = "lockstat")]
#[test]
fn stats_objects_outlive_reclamation() {
    let mut d = Donation::new();
    d.register(0, prio(10)).unwrap();
    d.register(1, prio(2)).unwrap();
    d.acquire(7, 0).unwrap();
    d.block(7, 1).unwrap();
    assert_eq!(d.wake_one(7), Some(1)); // handoff
    assert_eq!(d.wake_one(7), None); // released; the turnstile is reclaimed
    assert!(d.objects().next().is_none());
    let objects: vec::Vec<ObjectId> = d.stats_objects().collect();
    assert_eq!(objects, vec![7]);
    assert_eq!(d.stats(7).wait_entries, 1);
}

#[test]
fn end_call_no_alloc_matches_full_refresh() {
    let mut d = Donation::new();
    d.register(0, prio(20)).unwrap(); // server, owns lock 9
    d.register(1, prio(5)).unwrap(); // client
    d.register(2, prio(10)).unwrap(); // waiter on the server's lock
    d.begin_call(1, 0).unwrap();
    d.acquire(9, 0).unwrap();
    d.block(9, 2).unwrap();
    assert_eq!(d.effective_priority(0), Some(prio(5)));

    d.end_call_no_alloc(1, 0).expect("edge removal");
    assert!(!d.is_dirty(), "three-thread graph fits the bound");
    assert_eq!(d.effective_priority(0), Some(prio(10)));
    // The global recomputation must agree with the allocation-free local one.
    d.check_invariants()
        .expect("invariant after allocation-free end_call");

    assert_eq!(d.end_call_no_alloc(1, 0), Err(DonationError::StaleCall));
}

#[test]
fn end_call_no_alloc_marks_wide_graphs_dirty() {
    // A 65-deep call chain: removing the head edge affects 65 dependents, one more than the
    // allocation-free bound, so the graph is marked dirty instead of allocating in teardown.
    const CHAIN: u32 = 65;
    let mut d = Donation::new();
    for thread in 0..=CHAIN {
        d.register(thread, prio(u8::try_from(thread).unwrap()))
            .unwrap();
    }
    for thread in 0..CHAIN {
        d.begin_call(thread, thread + 1).unwrap();
    }
    d.end_call_no_alloc(0, 1).expect("edge removal");
    assert!(d.is_dirty(), "wide dependent closure must be deferred");
    d.refresh_if_dirty();
    assert!(!d.is_dirty());
    d.check_invariants()
        .expect("invariant after deferred refresh");
}

#[test]
fn session_thread_exit_releases_donation_entries() {
    // One SSH session registers four graph threads (client, server, lock owner, waker). The
    // session's thread-exit teardown must drop every registered/effective entry; at +4 entries
    // per session the 3072-byte map class fills at session 56.
    const SESSIONS: u32 = 58;
    const PER_SESSION: u32 = 4;

    let mut d = Donation::new();
    for session in 0..SESSIONS {
        let first = session * PER_SESSION;
        for offset in 0..PER_SESSION {
            d.register(first + offset, prio(10 + offset as u8)).unwrap();
        }
        d.begin_call(first + 1, first).unwrap();
        d.begin_call(first + 3, first + 2).unwrap();

        // A session thread exits: the graph unregister is the teardown boundary that the old
        // code lacked (it removed call edges only).
        for offset in 0..PER_SESSION {
            assert!(d.unregister(first + offset));
        }

        assert!(
            d.call_edges().is_empty(),
            "session {session}: call edge retained after thread exit"
        );
        for offset in 0..PER_SESSION {
            let thread = first + offset;
            assert_eq!(
                d.base_priority(thread),
                None,
                "session {session}: registered entry for thread {thread} retained after exit"
            );
            assert_eq!(
                d.effective_priority(thread),
                None,
                "session {session}: effective entry for thread {thread} retained after exit"
            );
        }
        d.check_invariants()
            .expect("invariant after session thread exit");
    }

    // Id reuse: a new thread with the same id starts from its own base, never the dead thread's.
    d.register(0, prio(2)).unwrap();
    assert_eq!(d.base_priority(0), Some(prio(2)));
    assert_eq!(d.effective_priority(0), Some(prio(2)));
    assert!(d.unregister(0));
    assert_eq!(d.base_priority(0), None);
    assert_eq!(d.effective_priority(0), None);
    d.check_invariants().expect("invariant after id reuse");
}

#[test]
fn unregister_restores_boost_and_clears_owned_state() {
    let mut d = Donation::new();
    d.register(0, prio(20)).unwrap(); // lock owner
    d.register(1, prio(0)).unwrap(); // high-priority waiter, also owns 9 and calls 2
    d.register(2, prio(10)).unwrap(); // server
    d.register(3, prio(5)).unwrap(); // second client of 2
    d.acquire(7, 0).unwrap();
    d.block(7, 1).unwrap();
    d.acquire(9, 1).unwrap();
    d.begin_call(1, 2).unwrap();
    d.begin_call(3, 2).unwrap();
    assert_eq!(d.effective_priority(0), Some(prio(0)));
    assert_eq!(d.effective_priority(2), Some(prio(0)));

    assert!(d.unregister(1));
    // The waiter's boost on 0 and call edge to 2 are gone; 2 keeps the surviving client's 5.
    assert_eq!(d.effective_priority(0), Some(prio(20)));
    assert_eq!(d.effective_priority(2), Some(prio(5)));
    assert_eq!(d.effective_priority(3), Some(prio(5)));
    assert_eq!(d.base_priority(1), None);
    assert_eq!(d.effective_priority(1), None);
    assert_eq!(d.waiting_on(1), None);
    assert!(d.waiters(7).is_empty());
    assert_eq!(d.owner(9), None);
    assert_eq!(d.call_edges(), vec![(3, 2)]);
    // Idempotent: a second exit cleanup finds no state and does not disturb the graph.
    assert!(!d.unregister(1));
    d.check_invariants().expect("invariant after unregister");
}
