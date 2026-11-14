// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

use super::*;

#[test]
fn the_first_allocation_is_aligned() {
    let mut alloc = LdAlloc::new();
    let at = alloc.alloc(1, 16).expect("alloc");
    assert_eq!(at % 16, 0);
}

#[test]
fn allocations_do_not_overlap() {
    let mut alloc = LdAlloc::new();
    let first = alloc.alloc(16, 16).expect("first");
    let second = alloc.alloc(16, 16).expect("second");
    assert!(second >= first + 16);
}

#[test]
fn a_freed_block_is_reused() {
    let mut alloc = LdAlloc::new();
    let first = alloc.alloc(32, 16).expect("first");
    assert!(alloc.free(first, 32));
    let second = alloc.alloc(16, 16).expect("second");
    assert_eq!(second, first);
}

#[test]
fn a_double_free_is_rejected() {
    let mut alloc = LdAlloc::new();
    let first = alloc.alloc(16, 16).expect("first");
    assert!(alloc.free(first, 16));
    assert!(!alloc.free(first, 16));
}

#[test]
fn a_zero_allocation_is_rejected() {
    let mut alloc = LdAlloc::new();
    assert!(alloc.alloc(0, 16).is_none());
}

#[test]
fn the_peak_tracks_the_live_bytes() {
    let mut alloc = LdAlloc::new();
    let first = alloc.alloc(64, 16).expect("first");
    assert!(alloc.peak() >= 64);
    assert!(alloc.free(first, 64));
    assert_eq!(alloc.live(), 0);
    assert!(alloc.peak() >= 64);
}

#[test]
fn the_bootstrap_pool_is_finite() {
    let mut alloc = LdAlloc::new();
    let mut count = 0usize;
    while alloc.alloc(1024, 16).is_some() {
        count = count.saturating_add(1);
        if count > 64 {
            break;
        }
    }
    assert!(alloc.failed());
}

#[test]
fn aligned_large_blocks_fit_or_fail_cleanly() {
    let mut alloc = LdAlloc::new();
    for size in [1usize, 15, 16, 17, 1024, 8192] {
        let _ = alloc.alloc(size, 64);
    }
}

#[test]
fn freeing_an_unknown_block_does_not_panic() {
    let mut alloc = LdAlloc::new();
    assert!(alloc.free(1234, 16));
}

#[test]
fn the_default_is_empty() {
    let alloc = LdAlloc::default();
    assert_eq!(alloc.live(), 0);
    assert_eq!(alloc.peak(), 0);
    assert!(!alloc.failed());
}
