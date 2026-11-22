// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Byte-set reference checks independent of the product scoreboard.

use tcp_sack_model::SackedSet;

#[test]
fn overlapping_and_adjacent_blocks_merge_without_losing_bytes() {
    let mut set = SackedSet::new(100);
    set.insert(10, 20);
    set.insert(15, 30);
    set.insert(30, 40);
    assert_eq!(set.runs(), vec![(10, 40)]);
    assert_eq!(set.gaps(), vec![(0, 10), (40, 100)]);
    assert!(set.invariants_hold());
}

#[test]
fn a_block_contained_by_another_is_idempotent() {
    let mut set = SackedSet::new(100);
    set.insert(10, 40);
    set.insert(20, 30);
    set.insert(10, 40);
    assert_eq!(set.runs(), vec![(10, 40)]);
    assert_eq!(set.bytes.len(), 30);
}

#[test]
fn cumulative_ack_trims_or_removes_ranges() {
    let mut set = SackedSet::new(100);
    set.insert(10, 20);
    set.insert(30, 40);
    set.ack(15);
    assert_eq!(set.runs(), vec![(15, 20), (30, 40)]);
    set.ack(35);
    assert_eq!(set.runs(), vec![(35, 40)]);
    set.ack(40);
    assert!(set.runs().is_empty());
}

#[test]
fn is_lost_uses_the_rfc6675_discontiguous_and_byte_thresholds() {
    let mut set = SackedSet::new(200);
    set.insert(100, 120);
    assert_eq!(set.ranges_above(0), 1);
    // With a 100-byte SMSS, these small blocks are below the byte threshold,
    // so only the discontiguous-block count decides.
    assert!(!set.is_lost(0, 3, 100));
    set.insert(150, 152);
    assert_eq!(set.ranges_above(0), 2);
    assert!(!set.is_lost(0, 3, 100));
    set.insert(160, 162);
    assert_eq!(set.ranges_above(0), 3);
    assert!(set.is_lost(0, 3, 100));
}

#[test]
fn pipe_and_lost_bytes_are_the_complement_of_sacked_bytes() {
    let mut set = SackedSet::new(100);
    set.insert(40, 60);
    // Both gaps are outstanding; HighRxt is zero here, so no retransmitted
    // tail is added.
    assert_eq!(set.pipe(0, 3, 10), 80);
    assert!(set.lost_ranges(3, 10).is_empty());
    assert_eq!(set.lost_bytes(3, 10), 0);
}

#[test]
fn the_reference_byte_count_is_strictly_above_the_point() {
    let mut set = SackedSet::new(100);
    set.insert(50, 70);
    assert_eq!(set.sacked_bytes_above(50), 19);
    assert_eq!(set.sacked_bytes_above(49), 20);
}
