// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

use super::*;
use core::sync::atomic::AtomicPtr;

#[repr(C)]
struct Buffer {
    head: Dtv,
    words: [AtomicUsize; 8],
}

impl Buffer {
    fn new(count: usize) -> Self {
        Self {
            head: Dtv {
                count: AtomicUsize::new(count),
            },
            words: [const { AtomicUsize::new(0) }; 8],
        }
    }

    fn set(&self, index: usize, value: usize) {
        self.words[index].store(value, Ordering::Release);
    }
}

#[test]
fn count_is_word_zero() {
    let buffer = Buffer::new(3);
    assert_eq!(buffer.head.count(), 3);
    assert_eq!(
        buffer.head.words(),
        &buffer.head as *const Dtv as *const AtomicUsize
    );
}

#[test]
fn lookup_reads_the_entry_and_adds_the_offset() {
    let buffer = Buffer::new(3);
    buffer.set(0, 0x1_0000);
    buffer.set(1, 0x2_0000);
    buffer.set(2, 0x3_0000);
    assert_eq!(lookup(&buffer.head, 1, 0), 0x1_0000);
    assert_eq!(lookup(&buffer.head, 2, 0x40), 0x2_0040);
    assert_eq!(lookup(&buffer.head, 3, 0), 0x3_0000);
}

#[test]
fn lookup_rejects_module_zero_and_out_of_range() {
    let buffer = Buffer::new(2);
    buffer.set(0, 0x1000);
    assert_eq!(lookup(&buffer.head, 0, 0), 0);
    assert_eq!(lookup(&buffer.head, 3, 0), 0);
    assert_eq!(
        try_lookup(&buffer.head, 0, 0),
        Err(crate::layout::LayoutError::TooLarge)
    );
    assert_eq!(
        try_lookup(&buffer.head, 3, 0),
        Err(crate::layout::LayoutError::TooLarge)
    );
    assert_eq!(try_lookup(&buffer.head, 1, 8), Ok(0x1008));
}

#[test]
fn a_null_slot_has_no_view() {
    let slot: AtomicPtr<Dtv> = AtomicPtr::new(core::ptr::null_mut());
    assert!(load(&slot).is_none());
}

#[test]
fn an_in_memory_view_loads() {
    let buffer = Buffer::new(1);
    buffer.set(0, 0x9000);
    let slot = AtomicPtr::new(&buffer.head as *const Dtv as *mut Dtv);
    let view = load(&slot).unwrap();
    assert_eq!(lookup(view, 1, 0), 0x9000);
}

#[test]
fn generation_monotonic_in_a_process() {
    let first = generation();
    crate::dtv::bump_generation();
    assert!(generation() > first);
}

#[test]
fn dtv_is_repr_c_with_one_word() {
    assert_eq!(core::mem::size_of::<Dtv>(), core::mem::size_of::<usize>());
    assert_eq!(core::mem::align_of::<Dtv>(), core::mem::align_of::<usize>());
}
