// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host tests for the fixture's bounded handshake helpers.

use super::*;

#[test]
fn deadline_is_not_reached_before_the_wall_clock_bound() {
    let start = 1_000_000_000;
    assert!(!deadline_reached(start, start));
    assert!(!deadline_reached(start, start + HANDSHAKE_TIMEOUT_NS - 1));
}

#[test]
fn deadline_is_reached_at_and_after_the_wall_clock_bound() {
    let start = 1_000_000_000;
    assert!(deadline_reached(start, start + HANDSHAKE_TIMEOUT_NS));
    assert!(deadline_reached(start, start + HANDSHAKE_TIMEOUT_NS + 1));
}

#[test]
fn state_snapshot_names_go_done_seen_exited_and_reader_tid() {
    let text = format!(
        "{}",
        StateSnapshot {
            go: 7,
            done: 6,
            seen: 0x2222_2222_2222_2222,
            exited: 2,
            reader_tid: 11,
        }
    );
    assert_eq!(
        text,
        "go=7 done=6 seen=0x2222222222222222 exited=2 reader_tid=11"
    );
}
