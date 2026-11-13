// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

use super::*;
use core::sync::atomic::Ordering;

#[test]
fn block_app_sigs_sets_every_app_bit_but_not_the_internal_ones() {
    let kernel = crate::test_support::MockKernel::new();
    let old = block_app_sigs(&kernel);
    assert_eq!(old, 0);
    let mask = current_mask(&kernel);
    assert_eq!(mask & INTERNAL_SIGNAL_BITS, 0);
    assert_eq!(mask & 1, 1);
    assert_eq!(mask & (1 << 63), 1 << 63);
}

#[test]
fn restore_puts_the_old_word_back() {
    let kernel = crate::test_support::MockKernel::new();
    let pthread = crate::thread::current_pthread(&kernel);
    pthread.sigmask.store(0x55, Ordering::Release);
    let old = block_app_sigs(&kernel);
    assert_eq!(old, 0x55);
    restore_sigs(&kernel, old);
    assert_eq!(current_mask(&kernel), 0x55);
}

#[test]
fn block_all_sigs_sets_every_bit() {
    let kernel = crate::test_support::MockKernel::new();
    block_all_sigs(&kernel);
    assert_eq!(current_mask(&kernel), u64::MAX);
}

#[test]
fn the_internal_bits_are_signals_32_to_34() {
    assert_eq!(INTERNAL_SIGNAL_BITS, 0b111 << 31);
    assert_eq!(INTERNAL_SIGNAL_BITS.count_ones(), 3);
}

#[test]
fn pending_is_zero_at_start() {
    let kernel = crate::test_support::MockKernel::new();
    assert_eq!(pending(&kernel), 0);
}
