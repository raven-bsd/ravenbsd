// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

use super::*;
use crate::host_ut::HostSigKernel;

#[test]
fn block_app_then_restore_is_two_atomic_operations() {
    let kernel = HostSigKernel::new();
    set_mask(&kernel, 0);
    let old = block_app(&kernel);
    assert_eq!(old, 0);
    assert_eq!(current_mask(&kernel), APP_MASK);
    restore(&kernel, old);
    assert_eq!(current_mask(&kernel), 0);
}

#[test]
fn block_all_sets_every_bit() {
    let kernel = HostSigKernel::new();
    set_mask(&kernel, 0);
    let old = block_all(&kernel);
    assert_eq!(old, 0);
    assert_eq!(current_mask(&kernel), ALL_MASK);
    restore(&kernel, 0);
}

#[test]
fn swap_mask_returns_the_previous_word() {
    let kernel = HostSigKernel::new();
    set_mask(&kernel, 0x10);
    let old = swap_mask(&kernel, 0x20);
    assert_eq!(old, 0x10);
    assert_eq!(current_mask(&kernel), 0x20);
}

#[test]
fn handler_mask_is_old_or_sa_or_sig_without_nodefer() {
    let old = 0x1u64;
    let sa = 0x2u64;
    let mask = handler_mask(old, sa, 10, 0);
    assert_eq!(mask & 0x1, 1);
    assert_eq!(mask & 0x2, 2);
    assert_eq!(
        mask & crate::critical::signal_bit(10),
        crate::critical::signal_bit(10)
    );
    assert_eq!(mask & APP_MASK, mask);
}

#[test]
fn sa_nodefer_does_not_add_the_signal() {
    let old = 0u64;
    let mask = handler_mask(old, 0, 10, crate::dispositions::SA_NODEFER);
    assert_eq!(mask & crate::critical::signal_bit(10), 0);
}

#[test]
fn internal_bits_never_survive_the_handler_mask() {
    let mask = handler_mask(u64::MAX, u64::MAX, 32, 0);
    assert_eq!(mask & APP_MASK, mask);
    assert_eq!(mask & 0x8000_0000, 0);
}

#[test]
fn deliverable_ignores_blocked_and_internal_bits() {
    let pending = 0xffff_ffff_ffff_ffffu64;
    assert_eq!(deliverable(pending, 0), APP_MASK);
    assert_eq!(deliverable(pending, APP_MASK), 0);
    let pending = crate::critical::signal_bit(9);
    assert_eq!(deliverable(pending, crate::critical::signal_bit(9)), 0);
    assert_eq!(deliverable(pending, 0), pending);
}

#[test]
fn is_blocked_and_set_from_word() {
    let mask = crate::critical::signal_bit(15) | !APP_MASK;
    assert!(is_blocked(mask, 15));
    assert!(!is_blocked(mask, 16));
    assert!(!is_blocked(mask, 0));
    let set = set_from_word(u64::MAX);
    assert_eq!(set.app_word(), APP_MASK);
}
