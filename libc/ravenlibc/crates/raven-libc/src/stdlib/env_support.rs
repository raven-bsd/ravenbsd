// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//!
//! The one process environment table of the shell (C41's `EnvTable`, C60c).
//!
//! C61's start stores the kernel `envp` array in the exported `environ`; the first environment
//! call adopts that array lazily (BP-1: no environment read at start). A mutation publishes a
//! new malloc'd pointer array and leaves the strings owned by C41 (deviation D-2: no string is
//! ever freed). A program that assigns `environ` itself is detected by pointer comparison and
//! the table is reloaded, never freed.

use alloc::vec::Vec;
use core::ffi::{c_char, c_void};

use raven_libc_abi::errno::Errno;
use raven_libc_stdlib::env::EnvTable;

static mut TABLE: EnvTable = EnvTable::new();
static mut READY: bool = false;
static mut LAST: *mut *mut c_char = core::ptr::null_mut();
static mut OWNED: bool = false;

/// The process environment table, adopting `environ` on first use.
///
/// # Safety
/// The caller must not hold a second `&mut` across a call.
pub(crate) unsafe fn table() -> &'static mut EnvTable {
    // SAFETY: one process table; the shell is single-threaded until C57.
    let current = unsafe { crate::stdlib::environ::environ };
    if !unsafe { READY } || current != unsafe { LAST } {
        // SAFETY: the static cells are only touched by this function.
        unsafe { reload(current) };
    }
    // SAFETY: as above; the reference lives for the process.
    unsafe { &mut *(&raw mut TABLE) }
}

/// Adopt the array a program currently publishes.
///
/// # Safety
/// `array` is NULL or a NULL-terminated array of C strings.
unsafe fn reload(array: *mut *mut c_char) {
    let mut entries: Vec<&'static [u8]> = Vec::new();
    if !array.is_null() {
        let mut index = 0usize;
        while index < 65_536 {
            // SAFETY: the caller's NULL-terminated array.
            let pointer = unsafe { *array.add(index) };
            if pointer.is_null() {
                break;
            }
            // SAFETY: each slot is a NUL-terminated string.
            entries.push(unsafe { crate::ffi::cstr::slice(pointer) });
            index = index.wrapping_add(1);
        }
    }
    // SAFETY: the process table cells.
    unsafe {
        (*&raw mut TABLE).adopt_initial(&entries);
        READY = true;
        LAST = array;
        OWNED = false;
    }
}

/// Publish the table as a fresh `environ` array.
///
/// # Safety
/// The caller holds the table borrow from [`table`].
pub(crate) unsafe fn publish(env: &EnvTable) -> Result<(), Errno> {
    let entries = env.as_slice();
    let total = entries
        .len()
        .saturating_add(1)
        .saturating_mul(core::mem::size_of::<*mut c_char>());
    // SAFETY: the shell allocator returns `total` writable bytes or NULL.
    let array = unsafe { crate::malloc::abi_alloc(total) } as *mut *mut c_char;
    if array.is_null() {
        return Err(Errno(raven_libc_abi::errno::ENOMEM as u16));
    }
    let mut index = 0usize;
    while index < entries.len() {
        if let Some(entry) = entries.get(index) {
            // SAFETY: the array has `entries.len() + 1` slots.
            unsafe { *array.add(index) = entry.as_ptr().cast::<c_char>().cast_mut() };
        }
        index = index.wrapping_add(1);
    }
    // SAFETY: the last slot is the NULL terminator.
    unsafe { *array.add(entries.len()) = core::ptr::null_mut() };
    // SAFETY: the static cells.
    unsafe {
        if OWNED && !LAST.is_null() {
            crate::malloc::free::free(LAST.cast::<c_void>());
        }
        crate::stdlib::environ::environ = array;
        LAST = array;
        OWNED = true;
    }
    Ok(())
}

/// `getenv`: the value slice, or `None`.
pub(crate) fn get(name: &[u8]) -> Option<&'static [u8]> {
    // SAFETY: one table borrow for the call.
    unsafe { table() }.getenv(name)
}

/// `secure_getenv`: the value slice; `AT_SECURE` is never set on Raven (H-7).
pub(crate) fn secure(name: &[u8]) -> Option<&'static [u8]> {
    // SAFETY: one process table borrow for the call.
    let env = unsafe { table() };
    raven_libc_stdlib::env::secure_getenv(env, name, false)
}

/// `setenv` plus the array publication.
pub(crate) fn set(name: &[u8], value: &[u8], overwrite: bool) -> Result<(), Errno> {
    let env = unsafe { table() };
    env.setenv(name, value, overwrite)?;
    // SAFETY: `env` is the process table and the borrow ends here.
    unsafe { publish(env) }
}

/// `putenv` plus the array publication; the caller's storage stays the value (POSIX).
pub(crate) fn put(pointer: *mut c_char) -> Result<(), Errno> {
    let env = unsafe { table() };
    // SAFETY: the caller's NUL-terminated string contract.
    let bytes = unsafe { crate::ffi::cstr::slice(pointer) };
    // SAFETY: POSIX `putenv` keeps the caller's storage for the life of the process.
    let leaked: &'static [u8] = unsafe { core::slice::from_raw_parts(bytes.as_ptr(), bytes.len()) };
    env.putenv(leaked)?;
    // SAFETY: `env` is the process table and the borrow ends here.
    unsafe { publish(env) }
}

/// `unsetenv` plus the array publication.
pub(crate) fn unset(name: &[u8]) -> Result<(), Errno> {
    let env = unsafe { table() };
    env.unsetenv(name)?;
    // SAFETY: `env` is the process table and the borrow ends here.
    unsafe { publish(env) }
}

/// `clearenv` plus the array publication.
pub(crate) fn clear() -> Result<(), Errno> {
    let env = unsafe { table() };
    env.clearenv();
    // SAFETY: `env` is the process table and the borrow ends here.
    unsafe { publish(env) }
}
