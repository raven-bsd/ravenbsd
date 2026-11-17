// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `at_quick_exit(3)` and `quick_exit(3)`.

use core::cell::UnsafeCell;
use core::ffi::c_int;

use raven_libc_stdlib::exit::QuickExitTable;

struct Registry(UnsafeCell<QuickExitTable>);

// SAFETY: the shell is single-threaded until the thread support lands; registrations are serial.
unsafe impl Sync for Registry {}

static QUICK_EXIT: Registry = Registry(UnsafeCell::new(QuickExitTable::new()));

/// `at_quick_exit`: register a handler run by `quick_exit`.
#[unsafe(no_mangle)]
pub extern "C" fn at_quick_exit(function: extern "C" fn()) -> c_int {
    // SAFETY: single-threaded registry cell.
    let table = unsafe { &mut *QUICK_EXIT.0.get() };
    match table.register(function as usize) {
        Ok(()) => 0,
        Err(_) => 1,
    }
}

/// `quick_exit`: run the quick-exit handlers LIFO, then terminate.
#[unsafe(no_mangle)]
pub extern "C" fn quick_exit(status: c_int) -> ! {
    // SAFETY: single-threaded registry cell.
    let table = unsafe { &mut *QUICK_EXIT.0.get() };
    while let Some(entry) = table.pop_next() {
        if entry.func != 0 {
            // SAFETY: `at_quick_exit` stored a C function pointer.
            let function: extern "C" fn() = unsafe { core::mem::transmute(entry.func) };
            function();
        }
    }
    crate::stdlib::_Exit::_Exit(status)
}
