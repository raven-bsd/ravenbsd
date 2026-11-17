// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host tests for the ring: the operation contract, an interleaved reference model and a
//! million-operation stress run.

use std::collections::VecDeque;
use std::sync::atomic::AtomicU64;

use crate::ring::{Descriptor, Error, Ring};

fn storage(capacity: usize) -> Vec<AtomicU64> {
    (0..capacity).map(|_| AtomicU64::new(0)).collect()
}

#[test]
fn invalid_capacity_is_rejected() {
    assert!(matches!(Ring::new(&[]), Err(Error::InvalidCapacity)));
    assert!(matches!(
        Ring::new(&storage(0)),
        Err(Error::InvalidCapacity)
    ));
    assert!(matches!(
        Ring::new(&storage(3)),
        Err(Error::InvalidCapacity)
    ));
    assert!(Ring::new(&storage(1)).is_ok());
    assert!(Ring::new(&storage(64)).is_ok());
}

#[test]
fn submit_and_complete_round_trip_in_order() {
    let storage = storage(8);
    let ring = Ring::new(&storage).unwrap();
    for value in 0..8u64 {
        ring.submit(Descriptor::new(value)).unwrap();
    }
    assert_eq!(ring.len(), 8);
    for value in 0..8u64 {
        assert_eq!(ring.complete(), Some(Descriptor::new(value)));
    }
    assert!(ring.is_empty());
    assert_eq!(ring.complete(), None);
}

#[test]
fn wrap_reuses_slots_over_capacity() {
    let storage = storage(4);
    let ring = Ring::new(&storage).unwrap();
    // Producer/consumer in lockstep: the free-running counters advance 10 times, wrapping the
    // masked slot index three times while order is preserved.
    for value in 0..10u64 {
        ring.submit(Descriptor::new(value)).unwrap();
        assert_eq!(ring.complete(), Some(Descriptor::new(value)));
    }
    assert!(ring.is_empty());
}

#[test]
fn full_rejects_and_counts() {
    let storage = storage(2);
    let ring = Ring::new(&storage).unwrap();
    ring.submit(Descriptor::new(1)).unwrap();
    ring.submit(Descriptor::new(2)).unwrap();
    assert!(ring.is_full());
    assert_eq!(ring.submit(Descriptor::new(3)), Err(Error::Full));
    assert_eq!(ring.rejected(), 1);
    assert_eq!(ring.complete(), Some(Descriptor::new(1)));
    assert!(!ring.is_full());
    ring.submit(Descriptor::new(3)).unwrap();
    assert!(ring.is_full());
}

#[test]
fn batch_submit_and_complete() {
    let storage = storage(8);
    let ring = Ring::new(&storage).unwrap();
    let descriptors: Vec<Descriptor> = (0..5u64).map(Descriptor::new).collect();
    assert_eq!(ring.submit_batch(&descriptors), 5);
    assert_eq!(ring.submit_batch(&descriptors), 3); // only 3 slots free
    assert_eq!(ring.len(), 8);

    let mut out = [Descriptor::new(0); 4];
    assert_eq!(ring.complete_batch(&mut out), 4);
    assert_eq!(out.map(Descriptor::value), [0, 1, 2, 3]);
    assert_eq!(ring.complete_batch(&mut out), 4);
    assert_eq!(out.map(Descriptor::value), [4, 0, 1, 2]);
    assert!(ring.is_empty());
}

#[test]
fn cancel_marks_pending_descriptors() {
    let storage = storage(4);
    let ring = Ring::new(&storage).unwrap();
    ring.submit(Descriptor::new(1)).unwrap();
    ring.submit(Descriptor::new(2)).unwrap();
    ring.submit(Descriptor::new(3)).unwrap();

    assert_eq!(ring.cancel(2), 1);
    assert_eq!(ring.complete(), Some(Descriptor::new(1)));
    assert_eq!(ring.complete(), Some(Descriptor::cancelled(2)));
    assert_eq!(ring.complete(), Some(Descriptor::new(3)));
    assert_eq!(ring.complete(), None);
}

#[test]
fn cancel_does_not_affect_completed_entries() {
    let storage = storage(4);
    let ring = Ring::new(&storage).unwrap();
    ring.submit(Descriptor::new(1)).unwrap();
    ring.submit(Descriptor::new(1)).unwrap();
    assert_eq!(ring.complete(), Some(Descriptor::new(1)));
    assert_eq!(ring.cancel(1), 1); // only the still-pending entry
    assert_eq!(ring.complete(), Some(Descriptor::cancelled(1)));
}

#[test]
fn poison_is_terminal_and_drains() {
    let storage = storage(4);
    let ring = Ring::new(&storage).unwrap();
    ring.submit(Descriptor::new(7)).unwrap();
    ring.submit(Descriptor::new(8)).unwrap();
    ring.poison();
    assert!(ring.is_poisoned());
    assert_eq!(ring.submit(Descriptor::new(9)), Err(Error::Poisoned));
    // The consumer still drains what was queued before the poison.
    assert_eq!(ring.complete(), Some(Descriptor::new(7)));
    assert_eq!(ring.complete(), Some(Descriptor::new(8)));
    assert_eq!(ring.complete(), None);
}

#[test]
fn descriptor_packs_value_and_cancelled_bit() {
    let value = Descriptor::new(u64::MAX);
    assert_eq!(value.value(), (1u64 << 63) - 1);
    assert!(!value.is_cancelled());

    let cancelled = Descriptor::cancelled(5);
    assert_eq!(cancelled.value(), 5);
    assert!(cancelled.is_cancelled());
    assert_eq!(Descriptor::from_raw(cancelled.raw()), cancelled);
}

// --- interleaved reference model ---------------------------------------------

#[derive(Clone, Copy, Debug)]
enum Action {
    Submit(u64),
    Complete,
    Cancel(u64),
    Poison,
}

const ACTIONS: [Action; 6] = [
    Action::Submit(1),
    Action::Submit(2),
    Action::Complete,
    Action::Cancel(1),
    Action::Cancel(2),
    Action::Poison,
];

fn replay(sequence: &[Action]) {
    let storage = storage(2);
    let ring = Ring::new(&storage).unwrap();
    let mut reference: VecDeque<Descriptor> = VecDeque::new();
    let mut poisoned = false;

    for action in sequence {
        match *action {
            Action::Submit(value) => {
                let expected = if poisoned {
                    Err(Error::Poisoned)
                } else if reference.len() >= 2 {
                    Err(Error::Full)
                } else {
                    reference.push_back(Descriptor::new(value));
                    Ok(())
                };
                assert_eq!(ring.submit(Descriptor::new(value)), expected);
            }
            Action::Complete => {
                assert_eq!(ring.complete(), reference.pop_front());
            }
            Action::Cancel(value) => {
                let count = ring.cancel(value);
                let mut marked = 0usize;
                for descriptor in reference.iter_mut() {
                    if !descriptor.is_cancelled() && descriptor.value() == value {
                        *descriptor = Descriptor::cancelled(value);
                        marked += 1;
                    }
                }
                assert_eq!(count, marked);
            }
            Action::Poison => {
                ring.poison();
                poisoned = true;
            }
        }
        assert_eq!(ring.len(), reference.len());
        assert_eq!(ring.is_full(), reference.len() >= 2);
        assert_eq!(ring.is_empty(), reference.is_empty());
        assert_eq!(ring.is_poisoned(), poisoned);
    }
}

fn enumerate(depth: usize, sequence: &mut Vec<Action>) {
    replay(sequence);
    if depth == 0 {
        return;
    }
    for action in ACTIONS {
        sequence.push(action);
        enumerate(depth - 1, sequence);
        sequence.pop();
    }
}

#[test]
fn interleaved_operations_match_a_reference_model() {
    // All operation sequences up to depth 6 over a capacity-2 ring; every prefix is replayed
    // against a `VecDeque` reference and the observable state must agree.
    enumerate(6, &mut Vec::new());
}

// --- million-operation model -------------------------------------------------

#[test]
fn million_operation_model_preserves_order_and_wrap() {
    const TOTAL: u64 = 1_000_000;
    const CAPACITY: usize = 1024;

    let storage = storage(CAPACITY);
    let ring = Ring::new(&storage).unwrap();

    let mut produced = 0u64;
    let mut consumed = 0u64;
    while consumed < TOTAL {
        while produced.wrapping_sub(consumed) < CAPACITY as u64 && produced < TOTAL {
            ring.submit(Descriptor::new(produced)).unwrap();
            produced += 1;
        }
        while consumed < produced {
            let descriptor = ring.complete().expect("drain");
            assert!(!descriptor.is_cancelled());
            assert_eq!(descriptor.value(), consumed, "out of order");
            consumed += 1;
        }
    }
    assert_eq!(consumed, TOTAL);
    assert!(ring.is_empty());
    // The free-running counters advanced far beyond the capacity: slots were reused ~1000 times.
    assert!(produced > CAPACITY as u64 * 100);
}
