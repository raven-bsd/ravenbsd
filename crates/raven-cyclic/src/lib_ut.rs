// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Unit tests for the deadline timer core.

use super::*;
use alloc::collections::BTreeMap;
use alloc::vec;
use alloc::vec::Vec;

/// A backend that records every `program` call and returns a fixed time.
struct RecordingBackend {
    programmed: Vec<Option<u64>>,
    now: u64,
}

impl RecordingBackend {
    fn new(now: u64) -> Self {
        RecordingBackend {
            programmed: Vec::new(),
            now,
        }
    }
}

impl Backend for RecordingBackend {
    fn program(&mut self, deadline_ns: Option<u64>) {
        self.programmed.push(deadline_ns);
    }

    fn now_ns(&self) -> u64 {
        self.now
    }
}

fn spec(when: u64) -> CyclicSpec {
    CyclicSpec {
        when,
        interval: 0,
        level: Level::Normal,
        slack_ns: 0,
    }
}

fn fire_into<B: Backend>(cyc: &mut Cyclics<B>, now: u64) -> Vec<CyclicId> {
    let mut fired = Vec::new();
    cyc.on_interrupt(now, |id| fired.push(id));
    fired
}

#[test]
fn fires_in_deadline_order() {
    let mut cyc = Cyclics::new(RecordingBackend::new(0));
    cyc.add(spec(30));
    cyc.add(spec(10));
    cyc.add(spec(20));
    assert_eq!(fire_into(&mut cyc, 30), vec![1, 2, 0]);
    assert!(cyc.is_empty());
}

#[test]
fn equal_deadlines_fire_in_id_order() {
    let mut cyc = Cyclics::new(RecordingBackend::new(0));
    cyc.add(spec(10));
    cyc.add(spec(10));
    cyc.add(spec(10));
    assert_eq!(fire_into(&mut cyc, 10), vec![0, 1, 2]);
}

#[test]
fn one_shot_fires_once() {
    let mut cyc = Cyclics::new(RecordingBackend::new(0));
    cyc.add(spec(10));
    assert_eq!(fire_into(&mut cyc, 10), vec![0]);
    assert!(cyc.is_empty());
    assert_eq!(fire_into(&mut cyc, 20), Vec::<CyclicId>::new());
}

#[test]
fn periodic_refires() {
    let mut cyc = Cyclics::new(RecordingBackend::new(0));
    cyc.add(CyclicSpec {
        when: 5,
        interval: 5,
        level: Level::Normal,
        slack_ns: 0,
    });
    // Due at 5, 10, 15, 20 (and reinserted at 25 > now).
    assert_eq!(fire_into(&mut cyc, 20), vec![0, 0, 0, 0]);
    assert_eq!(cyc.next_deadline(), Some(25));
}

#[test]
fn cancel_prevents_fire() {
    let mut cyc = Cyclics::new(RecordingBackend::new(0));
    let keep = cyc.add(spec(10));
    let kill = cyc.add(spec(5));
    assert!(cyc.cancel(kill));
    assert_eq!(fire_into(&mut cyc, 10), vec![keep]);
    assert!(cyc.is_empty());
}

#[test]
fn cancel_is_idempotent_and_safe() {
    let mut cyc = Cyclics::new(RecordingBackend::new(0));
    let id = cyc.add(spec(10));
    assert!(cyc.cancel(id));
    assert!(!cyc.cancel(id)); // already cancelled
    assert!(!cyc.cancel(999)); // unknown
    // The cancelled cyclic is unlinked immediately and never fires.
    assert_eq!(fire_into(&mut cyc, 10), Vec::<CyclicId>::new());
    assert!(cyc.is_empty());
}

#[test]
fn cancel_reclaims_the_last_entry_immediately() {
    let mut cyc = Cyclics::new(RecordingBackend::new(0));
    let mut ids = Vec::new();
    // A cancelled timer that is the last entry disarms the backend. If it is retained, no
    // interrupt ever reaches its original deadline, so the heap grows without bound.
    for step in 0..1000u64 {
        let id = cyc.add(spec(1_000_000_000 + step));
        assert!(cyc.cancel(id));
        ids.push(id);
    }
    assert_eq!(cyc.len(), 0, "cancelled cyclics retained");
    assert!(cyc.is_empty());
    assert_eq!(cyc.next_deadline(), None);
    // The backend must not stay armed for a cancelled deadline.
    assert_eq!(cyc.backend().programmed.last(), Some(&None));
    // Cancellation stays idempotent after the entry is gone.
    assert!(!cyc.cancel(ids[0]));
}

#[test]
fn backend_programmed_to_next_deadline() {
    let mut cyc = Cyclics::new(RecordingBackend::new(0));
    cyc.add(spec(30));
    cyc.add(spec(10));
    // Reprograms only when the deadline changes: 30, then 10 (earlier), then fire -> 30, then None.
    cyc.on_interrupt(10, |_| {});
    assert_eq!(cyc.backend().programmed, vec![Some(30), Some(10), Some(30)]);
    cyc.on_interrupt(30, |_| {});
    assert_eq!(
        cyc.backend().programmed,
        vec![Some(30), Some(10), Some(30), None]
    );
}

#[test]
fn cancel_earliest_reprograms() {
    let mut cyc = Cyclics::new(RecordingBackend::new(0));
    let early = cyc.add(spec(10));
    cyc.add(spec(30));
    assert_eq!(cyc.backend().programmed, vec![Some(10)]);
    assert!(cyc.cancel(early));
    assert_eq!(cyc.backend().programmed, vec![Some(10), Some(30)]);
}

#[test]
fn idle_tick_halts_when_empty() {
    let mut cyc = Cyclics::new(RecordingBackend::new(0));
    assert_eq!(cyc.next_deadline(), None);
    // Nothing to program: the backend is disarmed, so an idle CPU halts instead of ticking.
    assert_eq!(cyc.backend().programmed, Vec::<Option<u64>>::new());
    // Idle-tick: with one far deadline, the backend is armed exactly once and no intermediate
    // fire happens before it.
    cyc.add(spec(1_000_000_000));
    assert_eq!(cyc.backend().programmed, vec![Some(1_000_000_000)]);
    assert_eq!(fire_into(&mut cyc, 500_000_000), Vec::<CyclicId>::new());
    assert_eq!(cyc.next_deadline(), Some(1_000_000_000));
}

#[test]
fn coalescing_ordered_merge() {
    // Deadlines 100, 105, 110 all become due by `now`; a single on_interrupt drains them in order.
    let mut cyc = Cyclics::new(RecordingBackend::new(0));
    cyc.add(CyclicSpec {
        when: 110,
        interval: 0,
        level: Level::Normal,
        slack_ns: 0,
    });
    cyc.add(CyclicSpec {
        when: 100,
        interval: 0,
        level: Level::Normal,
        slack_ns: 5,
    });
    cyc.add(CyclicSpec {
        when: 105,
        interval: 0,
        level: Level::Normal,
        slack_ns: 0,
    });
    assert_eq!(fire_into(&mut cyc, 110), vec![1, 2, 0]);
    assert!(cyc.is_empty());
}

#[test]
fn slack_and_level_are_carried() {
    let mut cyc = Cyclics::new(RecordingBackend::new(0));
    let id = cyc.add(CyclicSpec {
        when: 10,
        interval: 0,
        level: Level::High,
        slack_ns: 50_000,
    });
    let spec = cyc.spec(id).expect("live cyclic");
    assert_eq!(spec.level, Level::High);
    assert_eq!(spec.slack_ns, 50_000);
    assert_eq!(fire_into(&mut cyc, 10), vec![id]);
    assert_eq!(cyc.spec(id), None); // fired and gone
}

#[test]
fn level_aware_callback_receives_the_firing_spec() {
    let mut cyc = Cyclics::new(RecordingBackend::new(0));
    let high = cyc.add(CyclicSpec {
        when: 20,
        interval: 0,
        level: Level::High,
        slack_ns: 7,
    });
    let normal = cyc.add(CyclicSpec {
        when: 10,
        interval: 0,
        level: Level::Normal,
        slack_ns: 3,
    });
    let low = cyc.add(CyclicSpec {
        when: 15,
        interval: 0,
        level: Level::Low,
        slack_ns: 11,
    });
    let mut fired = Vec::new();
    cyc.on_interrupt_with(20, |id, spec| {
        fired.push((id, spec.level, spec.when, spec.slack_ns));
    });
    assert_eq!(
        fired,
        vec![
            (normal, Level::Normal, 10, 3),
            (low, Level::Low, 15, 11),
            (high, Level::High, 20, 7),
        ]
    );
    assert!(cyc.is_empty());
}

#[test]
fn delayed_ipi_drains_periodic_and_reports_each_deadline() {
    let mut cyc = Cyclics::new(RecordingBackend::new(0));
    let id = cyc.add(CyclicSpec {
        when: 5,
        interval: 5,
        level: Level::High,
        slack_ns: 0,
    });
    let mut fired = Vec::new();
    // One delayed interrupt at 20 must drain every missed period in order, not only the first.
    cyc.on_interrupt_with(20, |fired_id, spec| {
        fired.push((fired_id, spec.when, spec.level));
    });
    assert_eq!(
        fired,
        vec![
            (id, 5, Level::High),
            (id, 10, Level::High),
            (id, 15, Level::High),
            (id, 20, Level::High),
        ]
    );
    assert_eq!(cyc.next_deadline(), Some(25));
}

#[test]
fn per_cpu_cyclics_are_independent() {
    let mut cpu0 = Cyclics::new(RecordingBackend::new(0));
    let mut cpu1 = Cyclics::new(RecordingBackend::new(0));
    cpu0.add(spec(10));
    cpu1.add(spec(20));
    assert_eq!(cpu0.backend().programmed, vec![Some(10)]);
    assert_eq!(cpu1.backend().programmed, vec![Some(20)]);
    assert_eq!(fire_into(&mut cpu0, 10).len(), 1);
    assert_eq!(cpu0.backend().programmed, vec![Some(10), None]);
    // Firing cpu0 must not disturb cpu1's programmed deadline.
    assert_eq!(cpu1.backend().programmed, vec![Some(20)]);
    assert_eq!(cpu1.next_deadline(), Some(20));
}

#[test]
fn remote_cancel_before_the_ipi_does_not_fire() {
    let mut cyc = Cyclics::new(RecordingBackend::new(0));
    let id = cyc.add(spec(10));
    assert!(cyc.cancel(id));
    assert_eq!(cyc.backend().programmed, vec![Some(10), None]);
    assert_eq!(fire_into(&mut cyc, 10), Vec::<CyclicId>::new());
    assert!(cyc.is_empty());
}

fn xorshift(state: &mut u64) -> u64 {
    let mut s = *state;
    s ^= s << 13;
    s ^= s >> 7;
    s ^= s << 17;
    *state = s;
    s
}

fn reference_fire(map: &mut BTreeMap<CyclicId, (u64, u64)>, now: u64) -> Vec<CyclicId> {
    let mut fired = Vec::new();
    loop {
        let due = map
            .iter()
            .filter(|(_, (when, _))| *when <= now)
            .min_by_key(|(id, (when, _))| (*when, *id));
        let Some((&id, &(when, interval))) = due else {
            break;
        };
        map.remove(&id);
        fired.push(id);
        if interval != 0 {
            map.insert(id, (when.saturating_add(interval), interval));
        }
    }
    fired
}

#[test]
fn randomized_insert_cancel_fire_model() {
    let mut state = 0x1234_5678_9abc_def0u64;
    let mut cyc = Cyclics::new(RecordingBackend::new(0));
    let mut reference: BTreeMap<CyclicId, (u64, u64)> = BTreeMap::new();
    let mut next_id = 0u64;
    let mut now = 0u64;
    const MAX_LIVE: usize = 32;

    for _ in 0..2000 {
        let roll = xorshift(&mut state) % 10;
        if roll < 5 && reference.len() < MAX_LIVE {
            // Add (bounded live set, and intervals larger than the fire step so a periodic
            // cyclic fires at most once per fire step).
            let when = now + xorshift(&mut state) % 200;
            let interval = if xorshift(&mut state).is_multiple_of(4) {
                0
            } else {
                50 + xorshift(&mut state) % 100
            };
            let id = cyc.add(CyclicSpec {
                when,
                interval,
                level: Level::Normal,
                slack_ns: 0,
            });
            assert_eq!(id, next_id);
            reference.insert(id, (when, interval));
            next_id += 1;
        } else if roll < 8 {
            // Cancel a random live id, or prove an unknown id is a no-op.
            if !reference.is_empty() && xorshift(&mut state).is_multiple_of(2) {
                let ids: Vec<CyclicId> = reference.keys().copied().collect();
                let id = ids[xorshift(&mut state) as usize % ids.len()];
                let expected = reference.remove(&id).is_some();
                assert_eq!(cyc.cancel(id), expected);
            } else {
                assert!(!cyc.cancel(next_id + 1)); // unknown id
            }
        } else {
            // Fire.
            now += xorshift(&mut state) % 20;
            let expected = reference_fire(&mut reference, now);
            assert_eq!(fire_into(&mut cyc, now), expected);
        }
        // The next-deadline rule must match the reference min.
        let expected_next = reference.values().map(|(when, _)| *when).min();
        assert_eq!(cyc.next_deadline(), expected_next);
    }
}

#[test]
fn reinsert_keeps_periodic_deadline_order() {
    // Two periodic cyclics with different periods interleave in deadline order.
    let mut cyc = Cyclics::new(RecordingBackend::new(0));
    cyc.add(CyclicSpec {
        when: 0,
        interval: 3,
        level: Level::Normal,
        slack_ns: 0,
    });
    cyc.add(CyclicSpec {
        when: 1,
        interval: 2,
        level: Level::Normal,
        slack_ns: 0,
    });
    // Fire until now = 9: A fires at 0,3,6,9 and B at 1,3,5,7,9, interleaved by deadline.
    let mut fired = Vec::new();
    cyc.on_interrupt(9, |id| fired.push(id));
    assert_eq!(fired, vec![0, 1, 0, 1, 1, 0, 1, 0, 1]);
}
