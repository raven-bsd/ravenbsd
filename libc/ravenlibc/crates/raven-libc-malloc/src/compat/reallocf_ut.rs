// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

use super::*;
use crate::compat::testheap::TestHeap;

#[test]
fn failure_frees_the_original() {
    let heap = TestHeap::new();
    heap.realloc_result.set(0);
    let result = reallocf(&heap, 0x1000, 64);
    assert_eq!(result, 0);
    assert_eq!(heap.freed.get(), 0x1000);
}

#[test]
fn success_keeps_the_original_alive() {
    let heap = TestHeap::new();
    heap.realloc_result.set(0x2000);
    let result = reallocf(&heap, 0x1000, 64);
    assert_eq!(result, 0x2000);
    assert_eq!(heap.freed.get(), 0);
}

#[test]
fn null_and_zero_are_not_freed() {
    let heap = TestHeap::new();
    heap.realloc_result.set(0);
    assert_eq!(reallocf(&heap, 0, 64), 0);
    assert_eq!(reallocf(&heap, 0x1000, 0), 0);
    assert_eq!(heap.freed.get(), 0);
}
