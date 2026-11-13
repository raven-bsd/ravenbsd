// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

use super::*;

#[test]
fn at_u32_writes_the_slice() {
    let words = [0u32; 3];
    at_u32(&words, 0).store(11, core::sync::atomic::Ordering::Release);
    at_u32(&words, 2).store(33, core::sync::atomic::Ordering::Release);
    assert_eq!(
        at_u32(&words, 0).load(core::sync::atomic::Ordering::Acquire),
        11
    );
    assert_eq!(
        at_u32(&words, 2).load(core::sync::atomic::Ordering::Acquire),
        33
    );
}

#[test]
fn at_i32_uses_the_same_word() {
    let words = [0u32; 2];
    at_i32(&words, 1).store(-1, core::sync::atomic::Ordering::Release);
    assert_eq!(
        at_u32(&words, 1).load(core::sync::atomic::Ordering::Acquire),
        u32::MAX
    );
    assert_eq!(
        at_u32(&words, 1).load(core::sync::atomic::Ordering::Acquire),
        u32::MAX
    );
}

#[test]
fn usize_round_trip_in_the_last_words() {
    let mut words = [0u32; 10];
    store_usize(&mut words, 8, usize::MAX);
    assert_eq!(load_usize(&words, 8), usize::MAX);
    assert_eq!(words[8], u32::MAX);
    assert_eq!(words[9], u32::MAX);
}

#[test]
fn usize_store_does_not_touch_neighbours() {
    let mut words = [7u32; 10];
    store_usize(&mut words, 6, 0x1122_3344);
    assert_eq!(words[5], 7);
    assert_eq!(words[8], 7);
    assert_eq!(load_usize(&words, 6), 0x1122_3344);
}

#[test]
fn out_of_range_load_is_zero() {
    let words = [1u32; 2];
    assert_eq!(load_usize(&words, 10), 0);
}

#[test]
fn as_u32_and_as_i32_keep_the_bits() {
    let word = AtomicI32::new(-2);
    let unsigned = as_u32(&word);
    assert_eq!(
        unsigned.load(core::sync::atomic::Ordering::Acquire),
        0xffff_fffe
    );
    let back = crate::raw::as_i32(&AtomicU32::new(0xffff_fffe));
    assert_eq!(back.load(core::sync::atomic::Ordering::Acquire), -2);
}

#[test]
fn the_views_are_aligned() {
    let words = [0u32; 4];
    assert_eq!(at_u32(&words, 1) as *const AtomicU32 as usize % 4, 0);
    assert_eq!(at_i32(&words, 3) as *const AtomicI32 as usize % 4, 0);
}
