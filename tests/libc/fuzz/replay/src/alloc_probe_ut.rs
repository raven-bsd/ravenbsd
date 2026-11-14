// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

use super::*;

#[test]
fn mark_and_peak_track_a_large_allocation() {
    let mark = mark();
    let buffer = vec![0u8; 1 << 20];
    assert!(peak_since_mark() >= 1 << 20);
    assert!(live() >= mark + (1 << 20));
    drop(buffer);
    assert!(live() >= mark);
}

#[test]
fn note_round_trips_and_clears() {
    note(0x1234_5678_9abc_def0);
    assert_eq!(last_note(), 0x1234_5678_9abc_def0);
    clear_note();
    assert_eq!(last_note(), 0);
}

#[test]
fn a_new_mark_resets_the_peak_window() {
    let _buffer = vec![0u8; 2 << 20];
    let _ = mark();
    let _small = [0u8; 64];
    assert!(peak_since_mark() < 1 << 20);
}

#[test]
fn the_last_note_wins() {
    note(1);
    note(2);
    assert_eq!(last_note(), 2);
}

#[test]
fn zero_sized_allocations_are_harmless() {
    let mark = mark();
    let empty: Vec<u8> = Vec::new();
    assert_eq!(empty.len(), 0);
    assert_eq!(peak_since_mark(), 0);
    assert_eq!(live(), mark);
}
