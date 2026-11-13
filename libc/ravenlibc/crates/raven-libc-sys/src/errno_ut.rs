// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Unit tests for [`crate::errno`].

#![allow(clippy::indexing_slicing)]

use super::*;

use host::sync::Mutex;
use host::thread_local;

static LOCK: Mutex<()> = Mutex::new(());

thread_local! {
    static TLS_SLOT: core::cell::Cell<i32> = const { core::cell::Cell::new(0) };
}

unsafe fn tls_provider() -> *mut i32 {
    TLS_SLOT.with(|slot| slot.as_ptr())
}

#[test]
fn fallback_cell_works_without_a_provider() {
    let _guard = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    reset_errno_slot();
    set_errno(0);
    assert_eq!(errno(), 0);
    set_errno(38);
    assert_eq!(errno(), 38);
}

#[test]
fn provider_takes_over_both_directions() {
    let _guard = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    reset_errno_slot();
    // SAFETY: the provider returns this thread's live TLS cell.
    unsafe { install_errno_slot(tls_provider) };
    set_errno(11);
    assert_eq!(errno(), 11);
    TLS_SLOT.with(|slot| assert_eq!(slot.get(), 11));
    reset_errno_slot();
}

#[test]
fn installing_twice_is_harmless() {
    let _guard = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    reset_errno_slot();
    // SAFETY: the provider returns this thread's live TLS cell.
    unsafe {
        install_errno_slot(tls_provider);
        install_errno_slot(tls_provider);
    }
    set_errno(22);
    assert_eq!(errno(), 22);
    reset_errno_slot();
}

#[test]
fn two_threads_use_different_slots() {
    let _guard = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    reset_errno_slot();
    // SAFETY: the provider returns the calling thread's live TLS cell.
    unsafe { install_errno_slot(tls_provider) };

    let first = host::thread::spawn(|| {
        set_errno(1);
        host::thread::yield_now();
        errno()
    });
    let second = host::thread::spawn(|| {
        set_errno(2);
        host::thread::yield_now();
        errno()
    });
    assert_eq!(first.join().unwrap(), 1);
    assert_eq!(second.join().unwrap(), 2);
    reset_errno_slot();
}

#[test]
fn decode_covers_the_error_window() {
    assert_eq!(ret(0), Ok(0));
    assert_eq!(ret(-4095), Err(Errno(4095)));
    assert_eq!(ret(-4096), Ok(0xffff_ffff_ffff_f000));
    assert_eq!(ret(-4), Err(Errno::EINTR));
}

#[test]
fn ret_i32_rejects_values_above_i32() {
    assert_eq!(ret_i32(7), Ok(7));
    assert_eq!(ret_i32(0x8000_0000), Err(Errno::EINVAL));
}

#[test]
fn successful_value_is_positive() {
    assert_eq!(ret(42), Ok(42));
    assert_eq!(ret_i32(42), Ok(42));
}

#[test]
fn install_does_not_touch_the_fallback_on_success() {
    let _guard = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    reset_errno_slot();
    set_errno(9);
    // SAFETY: the provider returns this thread's live TLS cell.
    unsafe { install_errno_slot(tls_provider) };
    assert_eq!(errno(), 0, "the TLS slot starts at zero");
    reset_errno_slot();
}
