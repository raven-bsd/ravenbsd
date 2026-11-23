// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Tests for the K449 test-only metric line.

use super::format_line;

#[test]
fn format_line_names_every_counter_in_order() {
    let line = format_line(
        100, 200, 300, 400, 5, 7, 9, 11, 13, 15, 17, 19, 21, 23, 25, 27, 29, 31,
    );
    assert_eq!(
        line,
        "K449-METRICS heap_used=100 heap_free=200 frames_free=300 \
         frames_alloc=400 procs=5 threads=7 ofds=9 live_threads=11 reap=13 \
         slot_drops=15 slot_denies=17 stacks_fresh=19 stacks_cached=21 \
         stacks_dropped=23 sshd_procs=25 sshd_threads=27 dma_used=29 donations=31"
    );
}

#[test]
fn format_line_keeps_large_values_machine_readable() {
    let line = format_line(
        1 << 20,
        1 << 19,
        1 << 40,
        1 << 39,
        1234,
        4321,
        1 << 50,
        1,
        2,
        3,
        4,
        5,
        6,
        7,
        8,
        9,
        10,
        11,
    );
    assert!(line.starts_with("K449-METRICS heap_used=1048576 heap_free=524288 "));
    assert!(line.contains(" procs=1234 threads=4321 ofds=1125899906842624 "));
    assert!(line.ends_with(" sshd_procs=8 sshd_threads=9 dma_used=10 donations=11"));
}
