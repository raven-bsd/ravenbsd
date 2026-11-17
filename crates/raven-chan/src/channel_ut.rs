// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host tests for the channel state machine: the operation contract and the exhaustive
//! small-state exploration.

use crate::{Channel, Completion, Endpoint, Error, Limits};

fn payload(txid: u64) -> u64 {
    0x1000 + txid
}

fn reply_payload(txid: u64) -> u64 {
    0x2000 + txid
}

fn drain(at: Endpoint, channel: &mut Channel) -> Vec<Completion> {
    let mut out = Vec::new();
    while let Some(completion) = channel.pop_completion(at) {
        out.push(completion);
    }
    out
}

// --- basic contract ----------------------------------------------------------

#[test]
fn new_channel_is_empty_and_open() {
    let channel = Channel::new();
    assert!(!channel.both_closed());
    for at in Endpoint::ALL {
        assert_eq!(channel.queued(at), 0);
        assert_eq!(channel.received(at), 0);
        assert_eq!(channel.outstanding(at), 0);
        assert_eq!(channel.pending_completions(at), 0);
        assert!(!channel.is_closed(at));
        assert!(!channel.is_dead(at));
    }
}

#[test]
fn call_queues_in_send_order() {
    let mut channel = Channel::new();
    channel.call(Endpoint::A, 1, payload(1)).unwrap();
    channel.call(Endpoint::A, 2, payload(2)).unwrap();
    assert_eq!(channel.queued(Endpoint::B), 2);
    assert_eq!(channel.outstanding(Endpoint::A), 2);

    let first = channel.recv(Endpoint::B).unwrap();
    let second = channel.recv(Endpoint::B).unwrap();
    assert_eq!(first.txid, 1);
    assert_eq!(first.payload_id, payload(1));
    assert_eq!(second.txid, 2);
    assert_eq!(second.payload_id, payload(2));
}

#[test]
fn call_rejects_zero_txid() {
    let mut channel = Channel::new();
    assert_eq!(
        channel.call(Endpoint::A, 0, payload(0)),
        Err(Error::InvalidTxid)
    );
    assert_eq!(channel.outstanding(Endpoint::A), 0);
    assert_eq!(channel.queued(Endpoint::B), 0);
}

#[test]
fn call_rejects_duplicate_outstanding_txid() {
    let mut channel = Channel::new();
    channel.call(Endpoint::A, 7, payload(7)).unwrap();
    assert_eq!(
        channel.call(Endpoint::A, 7, payload(7)),
        Err(Error::DuplicateTxid)
    );
}

#[test]
fn call_rejects_when_peer_queue_full() {
    let mut channel = Channel::with_limits(Limits::new(1, 4));
    channel.call(Endpoint::A, 1, payload(1)).unwrap();
    assert_eq!(
        channel.call(Endpoint::A, 2, payload(2)),
        Err(Error::QueueFull)
    );
    // No completion, no outstanding entry for the rejected call.
    assert_eq!(channel.outstanding(Endpoint::A), 1);
    assert_eq!(channel.accepted_calls(Endpoint::A), 1);
    // Draining the queue frees a slot.
    channel.recv(Endpoint::B).unwrap();
    channel.call(Endpoint::A, 2, payload(2)).unwrap();
}

#[test]
fn call_rejects_when_in_flight_budget_exhausted_and_completions_must_be_reaped() {
    let mut channel = Channel::with_limits(Limits::new(8, 1));
    channel.call(Endpoint::A, 1, payload(1)).unwrap();
    channel.recv(Endpoint::B).unwrap();
    // One call outstanding.
    assert_eq!(
        channel.call(Endpoint::A, 2, payload(2)),
        Err(Error::InFlightFull)
    );
    // Reply completes it, but the unread completion still counts against the budget.
    channel.reply(Endpoint::B, 1, reply_payload(1)).unwrap();
    assert_eq!(
        channel.call(Endpoint::A, 2, payload(2)),
        Err(Error::InFlightFull)
    );
    // Reaping the completion frees the slot.
    assert_eq!(channel.pop_completion(Endpoint::A).unwrap().txid(), 1);
    channel.call(Endpoint::A, 2, payload(2)).unwrap();
}

#[test]
fn recv_empty_returns_would_block() {
    let mut channel = Channel::new();
    assert_eq!(channel.recv(Endpoint::B), Err(Error::WouldBlock));
}

#[test]
fn recv_after_own_close_returns_endpoint_closed() {
    let mut channel = Channel::new();
    channel.close(Endpoint::B);
    assert_eq!(channel.recv(Endpoint::B), Err(Error::EndpointClosed));
}

#[test]
fn recv_after_peer_close_returns_peer_closed() {
    let mut channel = Channel::new();
    channel.close(Endpoint::A);
    assert_eq!(channel.recv(Endpoint::B), Err(Error::PeerClosed));
}

#[test]
fn recv_after_peer_death_returns_peer_dead() {
    let mut channel = Channel::new();
    channel.died(Endpoint::A);
    assert_eq!(channel.recv(Endpoint::B), Err(Error::PeerDead));
}

#[test]
fn reply_completes_the_call_once_with_the_reply_payload() {
    let mut channel = Channel::new();
    channel.call(Endpoint::A, 3, payload(3)).unwrap();
    let received = channel.recv(Endpoint::B).unwrap();
    assert_eq!(received.txid, 3);

    channel.reply(Endpoint::B, 3, reply_payload(3)).unwrap();
    assert_eq!(channel.outstanding(Endpoint::A), 0);
    assert_eq!(channel.received(Endpoint::B), 0);

    let completions = drain(Endpoint::A, &mut channel);
    assert_eq!(completions.len(), 1);
    assert_eq!(
        completions[0],
        Completion::Replied {
            txid: 3,
            payload_id: reply_payload(3)
        }
    );
}

#[test]
fn reply_before_recv_is_no_such_call() {
    let mut channel = Channel::new();
    channel.call(Endpoint::A, 4, payload(4)).unwrap();
    assert_eq!(
        channel.reply(Endpoint::B, 4, reply_payload(4)),
        Err(Error::NoSuchCall)
    );
    assert!(channel.is_outstanding(Endpoint::A, 4));
}

#[test]
fn reply_to_unknown_txid_is_no_such_call() {
    let mut channel = Channel::new();
    assert_eq!(
        channel.reply(Endpoint::B, 99, reply_payload(99)),
        Err(Error::NoSuchCall)
    );
}

#[test]
fn reply_twice_is_no_such_call() {
    let mut channel = Channel::new();
    channel.call(Endpoint::A, 5, payload(5)).unwrap();
    channel.recv(Endpoint::B).unwrap();
    channel.reply(Endpoint::B, 5, reply_payload(5)).unwrap();
    assert_eq!(
        channel.reply(Endpoint::B, 5, reply_payload(5)),
        Err(Error::NoSuchCall)
    );
    assert_eq!(channel.pending_completions(Endpoint::A), 1);
}

#[test]
fn replies_may_be_out_of_order() {
    let mut channel = Channel::new();
    channel.call(Endpoint::A, 1, payload(1)).unwrap();
    channel.call(Endpoint::A, 2, payload(2)).unwrap();
    channel.recv(Endpoint::B).unwrap();
    channel.recv(Endpoint::B).unwrap();

    channel.reply(Endpoint::B, 2, reply_payload(2)).unwrap();
    channel.reply(Endpoint::B, 1, reply_payload(1)).unwrap();

    let completions = drain(Endpoint::A, &mut channel);
    assert_eq!(completions.len(), 2);
    assert_eq!(completions[0].txid(), 2);
    assert_eq!(completions[1].txid(), 1);
}

#[test]
fn completions_are_owned_by_the_caller() {
    let mut channel = Channel::new();
    channel.call(Endpoint::A, 1, payload(1)).unwrap();
    channel.recv(Endpoint::B).unwrap();
    channel.reply(Endpoint::B, 1, reply_payload(1)).unwrap();
    assert_eq!(channel.pending_completions(Endpoint::A), 1);
    assert_eq!(channel.pending_completions(Endpoint::B), 0);
}

#[test]
fn cancel_queued_call_removes_it_from_the_peer_queue() {
    let mut channel = Channel::new();
    channel.call(Endpoint::A, 1, payload(1)).unwrap();
    channel.call(Endpoint::A, 2, payload(2)).unwrap();
    channel.cancel(Endpoint::A, 1).unwrap();

    assert_eq!(channel.outstanding(Endpoint::A), 1);
    assert_eq!(channel.queued(Endpoint::B), 1);
    // Only call 2 remains; it is received first.
    assert_eq!(channel.recv(Endpoint::B).unwrap().txid, 2);
    assert_eq!(channel.recv(Endpoint::B), Err(Error::WouldBlock));

    let completions = drain(Endpoint::A, &mut channel);
    assert_eq!(completions, vec![Completion::Cancelled { txid: 1 }]);
}

#[test]
fn cancel_received_call_prevents_reply() {
    let mut channel = Channel::new();
    channel.call(Endpoint::A, 1, payload(1)).unwrap();
    channel.recv(Endpoint::B).unwrap();
    channel.cancel(Endpoint::A, 1).unwrap();

    assert_eq!(channel.received(Endpoint::B), 0);
    assert_eq!(
        channel.reply(Endpoint::B, 1, reply_payload(1)),
        Err(Error::NoSuchCall)
    );
    assert_eq!(
        drain(Endpoint::A, &mut channel),
        vec![Completion::Cancelled { txid: 1 }]
    );
}

#[test]
fn cancel_unknown_or_foreign_txid_is_no_such_call() {
    let mut channel = Channel::new();
    channel.call(Endpoint::A, 1, payload(1)).unwrap();
    assert_eq!(channel.cancel(Endpoint::A, 99), Err(Error::NoSuchCall));
    // Endpoint B did not issue the call; it cannot cancel it.
    assert_eq!(channel.cancel(Endpoint::B, 1), Err(Error::NoSuchCall));
    assert!(channel.is_outstanding(Endpoint::A, 1));
}

#[test]
fn timeout_is_distinct_and_prevents_reply() {
    let mut channel = Channel::new();
    channel.call(Endpoint::A, 1, payload(1)).unwrap();
    channel.recv(Endpoint::B).unwrap();
    channel.timeout(Endpoint::A, 1).unwrap();

    assert_eq!(channel.received(Endpoint::B), 0);
    assert_eq!(
        channel.reply(Endpoint::B, 1, reply_payload(1)),
        Err(Error::NoSuchCall)
    );
    assert_eq!(
        drain(Endpoint::A, &mut channel),
        vec![Completion::TimedOut { txid: 1 }]
    );
}

#[test]
fn close_completes_both_directions_with_peer_closed() {
    let mut channel = Channel::new();
    channel.call(Endpoint::A, 1, payload(1)).unwrap();
    channel.call(Endpoint::B, 2, payload(2)).unwrap();
    channel.recv(Endpoint::A).unwrap(); // B's call received by A
    channel.close(Endpoint::A);

    assert!(channel.is_closed(Endpoint::A));
    assert_eq!(channel.outstanding(Endpoint::A), 0);
    assert_eq!(channel.outstanding(Endpoint::B), 0);
    assert_eq!(
        channel.queued(Endpoint::A) + channel.received(Endpoint::A),
        0
    );

    assert_eq!(
        drain(Endpoint::A, &mut channel),
        vec![Completion::PeerClosed { txid: 1 }]
    );
    assert_eq!(
        drain(Endpoint::B, &mut channel),
        vec![Completion::PeerClosed { txid: 2 }]
    );

    // The survivor can no longer reach the closed endpoint.
    assert_eq!(
        channel.call(Endpoint::B, 3, payload(3)),
        Err(Error::PeerClosed)
    );
    assert_eq!(channel.recv(Endpoint::B), Err(Error::PeerClosed));
}

#[test]
fn close_is_idempotent_and_then_call_fails_endpoint_closed() {
    let mut channel = Channel::new();
    channel.call(Endpoint::A, 1, payload(1)).unwrap();
    channel.close(Endpoint::A);
    let snapshot = channel.clone();
    channel.close(Endpoint::A);
    assert_eq!(channel, snapshot);
    assert_eq!(
        channel.call(Endpoint::A, 2, payload(2)),
        Err(Error::EndpointClosed)
    );
}

#[test]
fn death_completes_all_calls_with_peer_dead() {
    let mut channel = Channel::new();
    channel.call(Endpoint::A, 1, payload(1)).unwrap();
    channel.recv(Endpoint::B).unwrap(); // B holds A's call
    channel.call(Endpoint::B, 2, payload(2)).unwrap(); // queued at A

    channel.died(Endpoint::A);

    assert!(channel.is_dead(Endpoint::A));
    assert!(channel.is_closed(Endpoint::A));
    assert_eq!(channel.received(Endpoint::B), 0);
    assert_eq!(channel.queued(Endpoint::A), 0);

    assert_eq!(
        drain(Endpoint::A, &mut channel),
        vec![Completion::PeerDied { txid: 1 }]
    );
    assert_eq!(
        drain(Endpoint::B, &mut channel),
        vec![Completion::PeerDied { txid: 2 }]
    );
}

#[test]
fn death_prevents_reply_and_new_calls() {
    let mut channel = Channel::new();
    channel.call(Endpoint::A, 1, payload(1)).unwrap();
    channel.recv(Endpoint::B).unwrap();
    channel.died(Endpoint::A);

    // The surviving server's late reply finds no call.
    assert_eq!(
        channel.reply(Endpoint::B, 1, reply_payload(1)),
        Err(Error::NoSuchCall)
    );
    // The survivor's new call fails with peer-dead; the dead endpoint is itself closed.
    assert_eq!(
        channel.call(Endpoint::B, 2, payload(2)),
        Err(Error::PeerDead)
    );
    assert_eq!(
        channel.call(Endpoint::A, 2, payload(2)),
        Err(Error::EndpointClosed)
    );
}

#[test]
fn death_is_idempotent() {
    let mut channel = Channel::new();
    channel.died(Endpoint::A);
    let snapshot = channel.clone();
    channel.died(Endpoint::A);
    assert_eq!(channel, snapshot);
}

#[test]
fn close_after_death_is_idempotent() {
    let mut channel = Channel::new();
    channel.died(Endpoint::A);
    let snapshot = channel.clone();
    channel.close(Endpoint::A);
    assert_eq!(channel, snapshot);
    assert!(channel.is_dead(Endpoint::A));
}

#[test]
fn both_closed_requires_both_endpoints() {
    let mut channel = Channel::new();
    assert!(!channel.both_closed());
    channel.close(Endpoint::A);
    assert!(!channel.both_closed());
    channel.close(Endpoint::B);
    assert!(channel.both_closed());

    let mut dead = Channel::new();
    dead.died(Endpoint::A);
    assert!(!dead.both_closed());
    dead.close(Endpoint::B);
    assert!(dead.both_closed());
}

#[test]
fn reply_recv_delivers_reply_then_the_next_call() {
    let mut channel = Channel::new();
    channel.call(Endpoint::A, 1, payload(1)).unwrap();
    channel.call(Endpoint::A, 2, payload(2)).unwrap();
    channel.recv(Endpoint::B).unwrap();

    let next = channel
        .reply_recv(Endpoint::B, 1, reply_payload(1))
        .unwrap()
        .unwrap();
    assert_eq!(next.txid, 2);
    assert!(channel.is_received(Endpoint::B, 2));

    // The reply completion was recorded.
    assert_eq!(
        drain(Endpoint::A, &mut channel),
        vec![Completion::Replied {
            txid: 1,
            payload_id: reply_payload(1)
        }]
    );
}

#[test]
fn reply_recv_with_no_queued_call_returns_none() {
    let mut channel = Channel::new();
    channel.call(Endpoint::A, 1, payload(1)).unwrap();
    channel.recv(Endpoint::B).unwrap();
    assert_eq!(
        channel.reply_recv(Endpoint::B, 1, reply_payload(1)),
        Ok(None)
    );
}

#[test]
fn reply_recv_failed_reply_does_not_dequeue() {
    let mut channel = Channel::new();
    channel.call(Endpoint::A, 1, payload(1)).unwrap();
    channel.call(Endpoint::A, 2, payload(2)).unwrap();
    // Reply to an unknown txid must fail and leave call 1 queued.
    assert_eq!(
        channel.reply_recv(Endpoint::B, 99, reply_payload(99)),
        Err(Error::NoSuchCall)
    );
    assert_eq!(channel.queued(Endpoint::B), 2);
    assert_eq!(channel.received(Endpoint::B), 0);
}

#[test]
fn txid_may_be_reused_after_completion() {
    let mut channel = Channel::new();
    channel.call(Endpoint::A, 1, payload(1)).unwrap();
    channel.recv(Endpoint::B).unwrap();
    channel.reply(Endpoint::B, 1, reply_payload(1)).unwrap();
    channel.pop_completion(Endpoint::A).unwrap();
    channel.call(Endpoint::A, 1, payload(1)).unwrap();
    assert_eq!(channel.recv(Endpoint::B).unwrap().txid, 1);
}

#[test]
fn accepted_accounting_matches_outstanding_pending_and_popped() {
    let mut channel = Channel::new();
    channel.call(Endpoint::A, 1, payload(1)).unwrap();
    channel.recv(Endpoint::B).unwrap();
    channel.reply(Endpoint::B, 1, reply_payload(1)).unwrap();

    assert_eq!(channel.accepted_calls(Endpoint::A), 1);
    assert_eq!(channel.completed_calls(Endpoint::A), 1);
    assert_eq!(channel.pending_completions(Endpoint::A), 1);
    assert_eq!(channel.popped_completions(Endpoint::A), 0);

    channel.pop_completion(Endpoint::A).unwrap();
    assert_eq!(channel.popped_completions(Endpoint::A), 1);
    assert_eq!(channel.pending_completions(Endpoint::A), 0);
    assert_eq!(
        channel.accepted_calls(Endpoint::A),
        channel.outstanding(Endpoint::A)
            + channel.pending_completions(Endpoint::A)
            + channel.popped_completions(Endpoint::A)
    );
}

#[test]
fn payload_ids_are_opaque_and_preserved() {
    let mut channel = Channel::new();
    channel.call(Endpoint::B, 42, 0xDEAD_BEEF).unwrap();
    assert_eq!(channel.recv(Endpoint::A).unwrap().payload_id, 0xDEAD_BEEF);
    channel.reply(Endpoint::A, 42, 0xCAFE_F00D).unwrap();
    assert_eq!(
        drain(Endpoint::B, &mut channel),
        vec![Completion::Replied {
            txid: 42,
            payload_id: 0xCAFE_F00D
        }]
    );
}

// --- exhaustive small-state exploration -------------------------------------

/// Number of distinct `txid`s in the bounded model.
const TXIDS: [u64; 3] = [1, 2, 3];

fn txid_index(txid: u64) -> usize {
    (txid - 1) as usize
}

#[derive(Clone, Copy, Debug)]
enum Action {
    Call { from: Endpoint, txid: u64 },
    Recv { at: Endpoint },
    Reply { at: Endpoint, txid: u64 },
    ReplyRecv { at: Endpoint, txid: u64 },
    Cancel { from: Endpoint, txid: u64 },
    Timeout { from: Endpoint, txid: u64 },
    Close { at: Endpoint },
    Died { at: Endpoint },
}

#[derive(Clone, Debug)]
struct Node {
    chan: Channel,
    /// Which endpoint issued each `TXIDS[i]`, once it has been issued.
    owner: [Option<Endpoint>; TXIDS.len()],
    depth: usize,
}

fn actions(node: &Node) -> Vec<Action> {
    let mut out = Vec::new();
    for from in Endpoint::ALL {
        for (i, &txid) in TXIDS.iter().enumerate() {
            if node.owner[i].is_none() {
                out.push(Action::Call { from, txid });
            }
        }
        out.push(Action::Recv { at: from });
        for &txid in &TXIDS {
            out.push(Action::Reply { at: from, txid });
            out.push(Action::ReplyRecv { at: from, txid });
            out.push(Action::Cancel { from, txid });
            out.push(Action::Timeout { from, txid });
        }
        out.push(Action::Close { at: from });
        out.push(Action::Died { at: from });
    }
    out
}

fn apply(node: &Node, action: Action) -> (Node, Result<(), Error>) {
    let mut next = node.clone();
    next.depth += 1;
    let result = match action {
        Action::Call { from, txid } => next.chan.call(from, txid, payload(txid)),
        Action::Recv { at } => next.chan.recv(at).map(|_| ()),
        Action::Reply { at, txid } => next.chan.reply(at, txid, reply_payload(txid)),
        Action::ReplyRecv { at, txid } => next
            .chan
            .reply_recv(at, txid, reply_payload(txid))
            .map(|_| ()),
        Action::Cancel { from, txid } => next.chan.cancel(from, txid),
        Action::Timeout { from, txid } => next.chan.timeout(from, txid),
        Action::Close { at } => {
            next.chan.close(at);
            Ok(())
        }
        Action::Died { at } => {
            next.chan.died(at);
            Ok(())
        }
    };
    if let (Action::Call { from, txid }, Ok(())) = (action, result) {
        next.owner[txid_index(txid)] = Some(from);
    }
    (next, result)
}

fn call_error(before: &Channel, from: Endpoint) -> Error {
    let peer = from.peer();
    if before.is_closed(from) {
        Error::EndpointClosed
    } else if before.is_dead(peer) {
        Error::PeerDead
    } else if before.is_closed(peer) {
        Error::PeerClosed
    } else if before.outstanding(from) + before.pending_completions(from)
        >= before.limits().in_flight
    {
        Error::InFlightFull
    } else {
        Error::QueueFull
    }
}

fn recv_error(before: &Channel, at: Endpoint) -> Error {
    let peer = at.peer();
    if before.is_closed(at) {
        Error::EndpointClosed
    } else if before.is_dead(peer) {
        Error::PeerDead
    } else if before.is_closed(peer) {
        Error::PeerClosed
    } else {
        Error::WouldBlock
    }
}

fn reply_error(before: &Channel, at: Endpoint) -> Error {
    if before.is_closed(at) {
        Error::EndpointClosed
    } else {
        Error::NoSuchCall
    }
}

fn assert_failed_all(before: &Channel, after: &Channel, died: bool) {
    for endpoint in Endpoint::ALL {
        assert_eq!(after.outstanding(endpoint), 0);
        assert_eq!(after.queued(endpoint), 0);
        assert_eq!(after.received(endpoint), 0);
        for txid in before.outstanding_txids(endpoint) {
            let matched = after.completions(endpoint).any(|completion| {
                completion.txid() == txid
                    && if died {
                        matches!(completion, Completion::PeerDied { .. })
                    } else {
                        matches!(completion, Completion::PeerClosed { .. })
                    }
            });
            assert!(matched, "in-flight txid {txid} not completed");
        }
        assert_eq!(
            after.pending_completions(endpoint) - before.pending_completions(endpoint),
            before.outstanding(endpoint)
        );
    }
}

fn check_invariants(chan: &Channel, owner: &[Option<Endpoint>]) {
    let limits = chan.limits();
    for at in Endpoint::ALL {
        let peer = at.peer();
        assert!(chan.queued(at) <= limits.receive_queue);
        assert!(chan.outstanding(at) <= limits.in_flight);
        assert_eq!(
            chan.accepted_calls(at),
            chan.outstanding(at) + chan.pending_completions(at) + chan.popped_completions(at)
        );
        assert_eq!(
            chan.completed_calls(at),
            chan.pending_completions(at) + chan.popped_completions(at)
        );
        // Every in-flight call from `peer` sits in `at`'s queue or received table.
        assert_eq!(
            chan.queued(at) + chan.received(at),
            chan.outstanding(peer),
            "cross-endpoint accounting at {at:?}"
        );
    }
    for (i, &issuer) in owner.iter().enumerate() {
        let txid = TXIDS[i];
        let Some(from) = issuer else {
            for at in Endpoint::ALL {
                assert!(!chan.is_outstanding(at, txid));
                assert!(!chan.is_received(at, txid));
                assert!(chan.completions(at).all(|c| c.txid() != txid));
            }
            continue;
        };
        let in_flight = chan.is_outstanding(from, txid);
        let completed = chan.completions(from).filter(|c| c.txid() == txid).count();
        assert_eq!(
            in_flight as usize + completed,
            1,
            "txid {txid} not exactly-once"
        );
        assert!(chan.completions(from.peer()).all(|c| c.txid() != txid));
    }
}

#[test]
fn exhaustive_small_model_saturates_with_exactly_once_and_no_reply_after_cancel_or_death() {
    // Bounded model: three distinct txids, receive queue 2, in-flight budget 3, no completion
    // reaping (so completed calls stay visible to the invariants). Every enabled action is
    // applied from every reachable state until the transition system closes.
    let root = Node {
        chan: Channel::with_limits(Limits::new(2, 3)),
        owner: [None; TXIDS.len()],
        depth: 0,
    };

    use std::collections::{HashSet, VecDeque};

    let mut seen: HashSet<(Channel, [Option<Endpoint>; TXIDS.len()])> = HashSet::new();
    let mut queue: VecDeque<Node> = VecDeque::new();
    seen.insert((root.chan.clone(), root.owner));
    queue.push_back(root);

    let mut states = 0usize;
    let mut transitions = 0usize;
    let mut max_depth = 0usize;
    const MAX_STATES: usize = 200_000;

    while let Some(node) = queue.pop_front() {
        states += 1;
        max_depth = max_depth.max(node.depth);
        check_invariants(&node.chan, &node.owner);

        for action in actions(&node) {
            transitions += 1;
            let before = node.chan.clone();
            let (next, result) = apply(&node, action);

            match action {
                Action::Call { from, txid } => match result {
                    Ok(()) => {
                        assert_eq!(node.owner[txid_index(txid)], None);
                        assert!(next.chan.is_outstanding(from, txid));
                    }
                    Err(error) => assert_eq!(error, call_error(&before, from)),
                },
                Action::Recv { at } => match result {
                    Ok(()) => {
                        assert!(before.queued(at) > 0);
                        assert_eq!(next.chan.queued(at), before.queued(at) - 1);
                        assert_eq!(next.chan.received(at), before.received(at) + 1);
                    }
                    Err(error) => {
                        assert_eq!(before.queued(at), 0);
                        assert_eq!(error, recv_error(&before, at));
                    }
                },
                Action::Reply { at, txid } => {
                    let was_received = before.is_received(at, txid);
                    match result {
                        Ok(()) => {
                            assert!(was_received);
                            assert_eq!(next.chan.received(at), before.received(at) - 1);
                            let caller = node.owner[txid_index(txid)].unwrap();
                            assert_eq!(
                                next.chan.pending_completions(caller),
                                before.pending_completions(caller) + 1
                            );
                            assert!(next.chan.completions(caller).any(|c| matches!(
                                c,
                                Completion::Replied { txid: t, payload_id: p }
                                    if t == txid && p == reply_payload(txid)
                            )));
                        }
                        Err(error) => {
                            assert!(!was_received, "reply succeeded where it must not");
                            assert_eq!(error, reply_error(&before, at));
                            assert_eq!(next.chan, before);
                        }
                    }
                }
                Action::ReplyRecv { at, txid } => {
                    let was_received = before.is_received(at, txid);
                    match result {
                        Ok(()) => {
                            assert!(was_received);
                            let dequeued = before.queued(at) > 0;
                            assert_eq!(
                                next.chan.received(at),
                                before.received(at) - 1 + usize::from(dequeued)
                            );
                            let caller = node.owner[txid_index(txid)].unwrap();
                            assert_eq!(
                                next.chan.pending_completions(caller),
                                before.pending_completions(caller) + 1
                            );
                        }
                        Err(error) => {
                            assert!(!was_received);
                            assert_eq!(error, reply_error(&before, at));
                            assert_eq!(next.chan, before);
                        }
                    }
                }
                Action::Cancel { from, txid } | Action::Timeout { from, txid } => {
                    let kind = if matches!(action, Action::Timeout { .. }) {
                        "timeout"
                    } else {
                        "cancel"
                    };
                    match result {
                        Ok(()) => {
                            assert!(before.is_outstanding(from, txid));
                            assert!(!next.chan.is_outstanding(from, txid));
                            assert!(!next.chan.is_received(from.peer(), txid));
                            let completed = next.chan.completions(from).any(|c| {
                                c.txid() == txid
                                    && if kind == "timeout" {
                                        matches!(c, Completion::TimedOut { .. })
                                    } else {
                                        matches!(c, Completion::Cancelled { .. })
                                    }
                            });
                            assert!(completed, "{kind} completion missing");
                        }
                        Err(error) => {
                            if before.is_closed(from) {
                                assert_eq!(error, Error::EndpointClosed);
                            } else {
                                assert!(!before.is_outstanding(from, txid));
                                assert_eq!(error, Error::NoSuchCall);
                            }
                            assert_eq!(next.chan, before);
                        }
                    }
                }
                Action::Close { at } => {
                    if before.is_closed(at) {
                        assert_eq!(next.chan, before);
                    } else {
                        assert!(next.chan.is_closed(at));
                        assert_failed_all(&before, &next.chan, false);
                    }
                }
                Action::Died { at } => {
                    if before.is_dead(at) {
                        assert_eq!(next.chan, before);
                    } else {
                        assert!(next.chan.is_dead(at));
                        assert_failed_all(&before, &next.chan, true);
                    }
                }
            }

            check_invariants(&next.chan, &next.owner);

            if seen.insert((next.chan.clone(), next.owner)) {
                assert!(states < MAX_STATES, "state space did not saturate");
                queue.push_back(next);
            }
        }
    }

    println!(
        "exhaustive model saturated: {states} states, {transitions} transitions, max depth {max_depth}"
    );
    assert!(states > 1);
}
