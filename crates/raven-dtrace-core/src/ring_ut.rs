// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Unit tests for the per-CPU ring.

use super::*;

fn record(sequence: u64) -> Record {
    Record {
        sequence,
        ..Record::default()
    }
}

#[test]
fn record_is_a_fixed_wire_size() {
    assert_eq!(size_of::<Record>(), 64);
    assert_eq!(size_of::<CpuStats>(), 32);
}

#[test]
fn a_new_ring_is_empty() {
    let ring = Ring::<4>::new();
    assert!(ring.is_empty());
    assert_eq!(ring.pop(), None);
    assert_eq!(
        ring.stats(),
        CpuStats {
            produced: 0,
            consumed: 0,
            dropped: 0,
            capacity: 4
        }
    );
}

#[test]
fn records_are_fifo() {
    let ring = Ring::<4>::new();
    assert!(ring.push(record(1)));
    assert!(ring.push(record(2)));
    assert_eq!(ring.pop().map(|r| r.sequence), Some(1));
    assert_eq!(ring.pop().map(|r| r.sequence), Some(2));
    assert_eq!(ring.pop(), None);
}

#[test]
fn a_full_ring_drops_the_new_record() {
    let ring = Ring::<2>::new();
    assert!(ring.push(record(1)));
    assert!(ring.push(record(2)));
    assert!(!ring.push(record(3)));
    let stats = ring.stats();
    assert_eq!(stats.produced, 2);
    assert_eq!(stats.dropped, 1);
    assert_eq!(ring.pop().map(|r| r.sequence), Some(1));
    assert_eq!(ring.pop().map(|r| r.sequence), Some(2));
}

#[test]
fn the_ring_wraps_after_the_consumer_catches_up() {
    let ring = Ring::<4>::new();
    for sequence in 0..12 {
        assert!(ring.push(record(sequence)));
        assert_eq!(ring.pop().map(|r| r.sequence), Some(sequence));
    }
    let stats = ring.stats();
    assert_eq!(stats.produced, 12);
    assert_eq!(stats.consumed, 12);
    assert_eq!(stats.dropped, 0);
    assert!(ring.is_empty());
}

#[test]
fn indices_wrap_after_a_drain() {
    let ring = Ring::<4>::new();
    for sequence in 0..3 {
        assert!(ring.push(record(sequence)));
    }
    for sequence in 0..3 {
        assert_eq!(ring.pop().map(|r| r.sequence), Some(sequence));
    }
    // The producer index is 3, so these pushes reuse slots 3, 0, 1 and 2.
    for sequence in 3..7 {
        assert!(ring.push(record(sequence)));
    }
    for sequence in 3..7 {
        assert_eq!(ring.pop().map(|r| r.sequence), Some(sequence));
    }
    assert_eq!(ring.stats().dropped, 0);
}

#[test]
fn reset_clears_records_and_counters() {
    let ring = Ring::<4>::new();
    assert!(ring.push(record(1)));
    ring.reset();
    assert!(ring.is_empty());
    assert_eq!(ring.pop(), None);
    assert_eq!(
        ring.stats(),
        CpuStats {
            produced: 0,
            consumed: 0,
            dropped: 0,
            capacity: 4
        }
    );
}

#[test]
fn independent_rings_do_not_share_state() {
    let first = Ring::<2>::new();
    let second = Ring::<2>::new();
    assert!(first.push(record(1)));
    assert!(second.push(record(2)));
    assert_eq!(first.pop().map(|r| r.sequence), Some(1));
    assert_eq!(second.pop().map(|r| r.sequence), Some(2));
}
