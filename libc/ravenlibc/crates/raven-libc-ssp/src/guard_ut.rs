// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

use super::*;
use core::cell::Cell;
use core::sync::atomic::Ordering;

struct Sink {
    global: Cell<usize>,
    tls: Cell<usize>,
}

impl Sink {
    fn new() -> Self {
        Self {
            global: Cell::new(0),
            tls: Cell::new(0),
        }
    }
}

impl CanarySink for Sink {
    fn set_global(&self, value: usize) {
        self.global.set(value);
    }
    fn set_tls(&self, value: usize) {
        self.tls.set(value);
    }
}

fn reset() {
    SSP_WEAK.store(false, Ordering::SeqCst);
    GUARD.store(0, Ordering::SeqCst);
}

#[test]
fn sixteen_random_bytes_are_preserved_except_byte_one() {
    reset();
    let random = [0x11u8; 16];
    let (value, source) = compute_guard(Some(&random));
    assert_eq!(source, GuardSource::Kernel);
    let bytes = value.to_le_bytes();
    assert_eq!(bytes[0], 0x11);
    assert_eq!(bytes[1], 0);
    assert_eq!(bytes[2], 0x11);
}

#[test]
fn eight_random_bytes_are_enough() {
    reset();
    let (value, source) = compute_guard(Some(&[0x22; 8]));
    assert_eq!(source, GuardSource::Kernel);
    assert_eq!(value.to_le_bytes()[1], 0);
}

#[test]
fn thirty_two_random_bytes_use_the_first_word() {
    reset();
    let (value, source) = compute_guard(Some(&[0x33; 32]));
    assert_eq!(source, GuardSource::Kernel);
    assert_ne!(value, 0);
}

#[test]
fn missing_random_uses_the_terminator() {
    reset();
    let (value, source) = compute_guard(None);
    assert_eq!(source, GuardSource::Terminator);
    let bytes = value.to_le_bytes();
    assert_eq!(bytes[0], 0);
    assert_eq!(bytes[1], 0);
    assert_eq!(bytes[2], b'\n');
    assert_eq!(bytes[3], 0xff);
}

#[test]
fn all_zero_random_uses_the_terminator() {
    reset();
    let (_, source) = compute_guard(Some(&[0u8; 16]));
    assert_eq!(source, GuardSource::Terminator);
}

#[test]
fn too_short_random_uses_the_terminator() {
    reset();
    let (_, source) = compute_guard(Some(&[0x44; 3]));
    assert_eq!(source, GuardSource::Terminator);
}

#[test]
fn init_writes_both_copies() {
    reset();
    let sink = Sink::new();
    let source = init_guard(Some(&[0x55; 16]), &sink);
    assert_eq!(source, GuardSource::Kernel);
    assert_eq!(sink.global.get(), sink.tls.get());
    assert_eq!(sink.global.get(), GUARD.load(Ordering::SeqCst));
}

#[test]
fn terminator_sets_the_weak_flag() {
    reset();
    let sink = Sink::new();
    let _ = init_guard(None, &sink);
    assert!(SSP_WEAK.load(Ordering::SeqCst));
}

#[test]
fn width_four_has_no_byte_one_clear() {
    reset();
    let (value, source) = compute_guard_width(Some(&[0x11, 0x22, 0x33, 0x44]), 4);
    assert_eq!(source, GuardSource::Kernel);
    assert_eq!(value as u32, 0x4433_2211);
}
