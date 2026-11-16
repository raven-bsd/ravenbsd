// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `memmove(3)`.
use core::ffi::c_void;

/// Copy `n` bytes, tolerating overlap.
///
/// # Safety
/// `dest` has `n` writable bytes and `src` `n` readable bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn memmove(dest: *mut c_void, src: *const c_void, n: usize) -> *mut c_void {
    let destination = dest.cast::<u8>();
    let source = src.cast::<u8>();
    // SAFETY: the caller's contract is the memmove(3) contract. Volatile byte accesses keep
    // the overlap direction explicit; the crate's raw helper loop is the subject of a separate
    // follow-up (its backward path is wrong for `dest > src`).
    unsafe {
        if (destination as usize) < (source as usize) {
            let mut index = 0usize;
            while index < n {
                core::ptr::write_volatile(
                    destination.add(index),
                    core::ptr::read_volatile(source.add(index)),
                );
                index = index.wrapping_add(1);
            }
        } else {
            let mut index = n;
            while index != 0 {
                index = index.wrapping_sub(1);
                core::ptr::write_volatile(
                    destination.add(index),
                    core::ptr::read_volatile(source.add(index)),
                );
            }
        }
    }
    dest
}
