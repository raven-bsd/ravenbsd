// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Tests for the K449 test-only metric line.

use super::format_line;

#[test]
fn format_line_names_every_counter_in_order() {
    let line = format_line(
        100, 200, 300, 400, 5, 7, 9, 11, 13, 15, 17, 19, 21, 23, 25, 27, 29, 31, 33, 35, 37, 39,
        41, 43, 45, 47, 49,
    );
    assert_eq!(
        line,
        "K449-METRICS heap_used=100 heap_free=200 frames_free=300 \
         frames_alloc=400 procs=5 threads=7 ofds=9 \
         pt_frames=11 stack_frames=13 init_pages=15 vfsd_pages=17 netd_pages=19 sshd_pages=21 \
         c3072_free=23 c3072_used=25 c3072_slabs=27 \
         live_threads=29 reap=31 slot_drops=33 slot_denies=35 stacks_fresh=37 \
         stacks_cached=39 stacks_dropped=41 sshd_procs=43 sshd_threads=45 dma_used=47 \
         donations=49"
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
        12,
        13,
        14,
        15,
        16,
        17,
        18,
        19,
        20,
    );
    assert!(line.starts_with("K449-METRICS heap_used=1048576 heap_free=524288 "));
    assert!(line.contains(" procs=1234 threads=4321 ofds=1125899906842624 "));
    assert!(line.ends_with(
        " pt_frames=1 stack_frames=2 init_pages=3 vfsd_pages=4 netd_pages=5 sshd_pages=6 \
         c3072_free=7 c3072_used=8 c3072_slabs=9 live_threads=10 reap=11 slot_drops=12 \
         slot_denies=13 stacks_fresh=14 stacks_cached=15 stacks_dropped=16 sshd_procs=17 \
         sshd_threads=18 dma_used=19 donations=20"
    ));
}
