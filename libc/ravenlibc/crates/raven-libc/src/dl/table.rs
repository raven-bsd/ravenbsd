// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! The installed loader table (layout copied from C62's `ld/src/table.rs`, first-party).

use core::sync::atomic::{AtomicPtr, Ordering};

/// The C ABI table installed into `libc.so.1` by the loader.
#[repr(C)]
pub(crate) struct RtldTable {
    pub(crate) version: u32,
    pub(crate) size: u32,
    pub(crate) dlopen: extern "C" fn(*const u8, i32) -> *mut u8,
    pub(crate) dlclose: extern "C" fn(*mut u8) -> i32,
    pub(crate) dlsym: extern "C" fn(*mut u8, *const u8, usize) -> *mut u8,
    pub(crate) dlvsym: extern "C" fn(*mut u8, *const u8, *const u8, usize) -> *mut u8,
    pub(crate) dlerror: extern "C" fn() -> *const u8,
    pub(crate) dladdr: extern "C" fn(usize, *mut u8) -> i32,
    pub(crate) dlinfo: extern "C" fn(*mut u8, i32, *mut u8) -> i32,
    pub(crate) dl_iterate_phdr:
        extern "C" fn(Option<extern "C" fn(*mut u8, usize, *mut u8) -> i32>, *mut u8) -> i32,
}

/// The process-wide installed table.
static DL_TABLE: AtomicPtr<RtldTable> = AtomicPtr::new(core::ptr::null_mut());

/// Read the installed table with Acquire ordering.
pub(crate) fn get() -> *const RtldTable {
    DL_TABLE.load(Ordering::Acquire)
}

/// The loader's install hook.
///
/// # Safety
/// `table` points to a live `RtldTable` owned by the loader for the process lifetime.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __rl_dl_install(table: *const RtldTable) {
    if table.is_null() {
        return;
    }
    // SAFETY: the caller contract says the table is live and readable.
    let candidate = unsafe { &*table };
    if candidate.version != 1 || candidate.size < core::mem::size_of::<RtldTable>() as u32 {
        return;
    }
    DL_TABLE.store(table.cast_mut(), Ordering::Release);
}
