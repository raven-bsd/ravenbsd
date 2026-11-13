// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

use super::*;

#[test]
fn mallinfo_layout_is_ten_ints() {
    assert_eq!(core::mem::size_of::<Mallinfo>(), 40);
    assert_eq!(core::mem::size_of::<Mallinfo2>(), 80);
}

#[test]
fn mallinfo_defaults_are_zero() {
    assert_eq!(
        Mallinfo::default(),
        Mallinfo {
            arena: 0,
            ordblks: 0,
            smblks: 0,
            hblks: 0,
            hblkhd: 0,
            usmblks: 0,
            fsmblks: 0,
            uordblks: 0,
            fordblks: 0,
            keepcost: 0
        }
    );
    assert_eq!(Mallinfo2::default().arena, 0);
}

#[test]
fn heap_is_object_safe() {
    let heap: &dyn Heap = &crate::compat::testheap::TestHeap::new();
    assert_eq!(heap.usable_size(1), 256);
    assert!(!heap.trim());
}
