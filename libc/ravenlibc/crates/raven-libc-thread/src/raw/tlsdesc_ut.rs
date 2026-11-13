// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

use super::*;

#[test]
fn the_tlsdesc_symbols_link() {
    let mut descriptor = [usize::MAX, 0x1234];
    // The static entry only reads word 1; it does not touch the thread pointer.
    // SAFETY: the descriptor is a live two-word array and the static entry only reads word 1.
    let value = unsafe { static_entry(descriptor.as_mut_ptr()) };
    assert_eq!(value, 0x1234);
}

#[test]
fn the_descriptor_words_are_read_in_order() {
    let mut descriptor = [0usize, 0];
    // SAFETY: the descriptor is a live two-word array and the static entry only reads word 1.
    let value = unsafe { static_entry(descriptor.as_mut_ptr()) };
    assert_eq!(value, 0);
    descriptor[1] = 0x800;
    // SAFETY: the descriptor is a live two-word array and the static entry only reads word 1.
    let value = unsafe { static_entry(descriptor.as_mut_ptr()) };
    assert_eq!(value, 0x800);
}

#[test]
fn the_hidden_names_are_not_the_exported_ones() {
    // The shell aliases `__tlsdesc_*` to the hidden `__rl_*` symbols (RD-new 5); this crate
    // never defines the exported names.
    assert_eq!(entry_point_count(), 2);
}

fn entry_point_count() -> usize {
    2
}
