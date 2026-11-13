// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// RavenBSD: C-ABI shell for the uuid_to_string export (ticket C65, `17` section 13). The logic is in
// `raven-libc-compat`; this file owns the C struct, the caller buffers and the errno write.
//! `uuid_to_string` C-ABI export (ticket C65).

#![allow(dead_code)]

use core::ffi::{c_char, c_int};

use raven_libc_compat::Entropy;
use raven_libc_compat::uuid::{self, Status, Uuid};

// The real entropy source: `arc4random_buf` (the rand crate, C50).
unsafe extern "C" {
    fn arc4random_buf(buffer: *mut core::ffi::c_void, length: usize);
}

struct RealEntropy;

impl Entropy for RealEntropy {
    fn fill(&mut self, buffer: &mut [u8]) {
        // SAFETY: `buffer` is a writable slice.
        unsafe { arc4random_buf(buffer.as_mut_ptr().cast(), buffer.len()) };
    }
}

unsafe fn read(pointer: *const core::ffi::c_void) -> Uuid {
    // SAFETY: the caller passes a 16-byte uuid_t.
    unsafe { core::ptr::read_unaligned(pointer.cast::<Uuid>()) }
}

unsafe fn write_uuid(pointer: *mut core::ffi::c_void, value: Uuid) {
    // SAFETY: the caller passes a writable 16-byte uuid_t.
    unsafe { core::ptr::write_unaligned(pointer.cast::<Uuid>(), value) };
}

unsafe fn set_status(status: *mut u32, value: Status) {
    if !status.is_null() {
        // SAFETY: the caller gives a writable status word.
        unsafe { *status = value as u32 };
    }
}

/// `uuid_to_string`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn uuid_to_string(
    pointer: *const core::ffi::c_void,
    string: *mut *mut c_char,
    status: *mut u32,
) {
    // SAFETY: `status` is null or writable.
    unsafe { set_status(status, Status::Ok) };
    if string.is_null() {
        return;
    }
    // SAFETY: null means nil.
    let value = if pointer.is_null() {
        Uuid::default()
    } else {
        unsafe { read(pointer) }
    };
    let mut text = [0u8; 37];
    uuid::to_string(&value, &mut text);
    let copy = unsafe { super::alloc_cstr(&text[..36]) };
    if copy.is_null() {
        // SAFETY: `status` is null or writable.
        unsafe { set_status(status, Status::NoMemory) };
        return;
    }
    // SAFETY: `string` is writable.
    unsafe { *string = copy };
}
