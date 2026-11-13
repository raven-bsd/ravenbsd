// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

use super::*;
use crate::compat::testheap::TestHeap;

#[test]
fn zeroes_then_frees() {
    let heap = TestHeap::new();
    let pointer = heap.address();
    raw::fill(pointer, 0xaa, 8);
    freezero(&heap, pointer, 8);
    let mut bytes = [1u8; 8];
    raw::read_bytes(pointer, &mut bytes);
    assert_eq!(bytes, [0; 8]);
    assert_eq!(heap.freed.get(), pointer);
}

#[test]
fn null_is_a_no_op() {
    let heap = TestHeap::new();
    freezero(&heap, 0, 8);
    assert_eq!(heap.freed.get(), 0);
}
