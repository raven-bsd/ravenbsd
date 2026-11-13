// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! Unit tests of the arena (`arena.rs`).

use super::*;

#[test]
fn a_new_arena_is_all_canary() {
    let arena = Arena::new(64);
    assert_eq!(arena.len(), 64);
    assert!(!arena.is_empty());
    assert_eq!(arena.read(0, 64), &[CANARY; 64]);
}

#[test]
fn write_read_round_trip_and_pointer_stability() {
    let mut arena = Arena::new(64);
    let first = arena.ptr(8);
    arena.write_u64(8, 0x1234_5678_9abc_def0);
    assert_eq!(arena.read_u64(8), 0x1234_5678_9abc_def0);
    // The allocation did not move: the same offset keeps the same address.
    assert_eq!(arena.ptr(8), first);
    arena.write_u32(4, 7);
    assert_eq!(arena.read_u32(4), 7);
    arena.write_i32(0, -1);
    assert_eq!(arena.read_u32(0), u32::MAX);
}

#[test]
fn fill_touches_only_the_named_range() {
    let mut arena = Arena::new(32);
    arena.fill(8, 8, PADDING);
    assert_eq!(arena.read(0, 8), &[CANARY; 8]);
    assert_eq!(arena.read(8, 8), &[PADDING; 8]);
    assert_eq!(arena.read(16, 16), &[CANARY; 16]);
}

#[test]
fn canaries_are_16_bytes_around_the_frame() {
    let arena = Arena::new(64);
    assert!(arena.before_canary_ok(16));
    assert!(arena.after_canary_ok(16, 32));
    assert!(!arena.before_canary_ok(8));
    let mut damaged = Arena::new(64);
    damaged.fill(48, 8, PADDING);
    assert!(!damaged.after_canary_ok(16, 32));
}

#[test]
fn a_pointer_inside_the_arena_is_writable_through_raw_pointer() {
    let mut arena = Arena::new(32);
    arena.write_u64(8, 0);
    let pointer = arena.ptr(16);
    // SAFETY: `pointer` is inside the live arena.
    unsafe { core::ptr::write_unaligned(pointer as *mut u64, 0xABCD) };
    assert_eq!(arena.read_u64(16), 0xABCD);
    // A pointer written into the frame stays readable as data.
    arena.write_ptr(0, pointer);
    assert_eq!(arena.read_u64(0) as usize, pointer);
}
