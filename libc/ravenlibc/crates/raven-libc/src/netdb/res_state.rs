// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `__res_state(3)`: the caller-owned resolver state pointer.
//!
//! C51's resolver has no global state (`res_init` is a no-op and `res_query` reads
//! `/etc/resolv.conf` itself). This export supplies the ABI-compatible writable state cell that
//! legacy callers dereference through `<resolv.h>`; it is zeroed and is not used to fake query
//! data.

use core::ffi::c_void;

/// A conservative, aligned backing store for the libc `struct __res_state`.
#[repr(C, align(16))]
struct Storage([u8; 2048]);

static mut STATE: Storage = Storage([0u8; 2048]);

/// `__res_state(3)`.
#[unsafe(no_mangle)]
pub extern "C" fn __res_state() -> *mut c_void {
    &raw mut STATE as *mut c_void
}

#[cfg(test)]
#[path = "res_state_ut.rs"]
mod tests;
