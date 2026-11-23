// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Tests for the K449 test-only metric line.

use super::format_line;

#[test]
fn format_line_names_every_counter_in_order() {
    let line = format_line(
        100, 200, 300, 400, 5, 7, 9, 11, 13, 17, 19, 23, 29, 31, 32, 33, 41, 43, 45, 47, 49, 51,
        53, 55, 57, 59, 61,
    );
    assert_eq!(
        line,
        "K449-METRICS heap_used=100 heap_free=200 frames_free=300 \
         frames_alloc=400 procs=5 threads=7 ofds=9 \
         pt_frames=11 stack_frames=13 init_pages=17 vfsd_pages=19 netd_pages=23 sshd_pages=29 \
         c3072_free=31 c3072_used=32 c3072_slabs=33 \
         live_threads=41 reap=43 slot_drops=45 slot_denies=47 stacks_fresh=49 \
         stacks_cached=51 stacks_dropped=53 sshd_procs=55 sshd_threads=57 \
         dma_used=59 donations=61"
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
        12,
        13,
        14,
        15,
        16,
        17,
        18,
        19,
        20,
        21,
    );
    assert!(line.starts_with("K449-METRICS heap_used=1048576 heap_free=524288 "));
    assert!(line.contains(" procs=1234 threads=4321 ofds=1125899906842624 "));
    assert!(line.ends_with(" sshd_procs=18 sshd_threads=19 dma_used=20 donations=21"));
}
