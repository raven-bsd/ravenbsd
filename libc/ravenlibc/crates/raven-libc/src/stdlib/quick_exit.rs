// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//!
//! `at_quick_exit(3)` and `quick_exit(3)`: the shell instance of C41's `QuickExitTable`.
//!
//! The table semantics (a LIFO list separate from `atexit`, consume-before-call) live in
//! `raven-libc-stdlib`; this file owns the process-global instance, the spin lock and the two C
//! entry points. No handler runs while the lock is held, so a handler may call
//! `at_quick_exit` again.

use core::cell::UnsafeCell;
use core::ffi::c_int;

use raven_libc_abi::errno::Errno;
use raven_libc_stdlib::exit::{Entry, QuickExitTable};

use crate::ffi::lock::SpinLock;

/// The quick-exit table and its lock.
struct Registry {
    /// Serialises every access to `table`.
    lock: SpinLock,
    /// C41's quick-exit table; only touched while `lock` is held.
    table: UnsafeCell<QuickExitTable>,
}

// SAFETY: `lock` serialises every access to the `UnsafeCell`; `with_table` is the only reader
// or writer and no handler runs inside it.
unsafe impl Sync for Registry {}

impl Registry {
    /// An empty registry.
    const fn new() -> Self {
        Self {
            lock: SpinLock::new(),
            table: UnsafeCell::new(QuickExitTable::new()),
        }
    }

    /// Run `visit` on the table with the lock held.
    fn with_table<R>(&self, visit: impl FnOnce(&mut QuickExitTable) -> R) -> R {
        self.lock.acquire();
        // SAFETY: the lock is held for the whole visit and released before any handler runs.
        let result = visit(unsafe { &mut *self.table.get() });
        self.lock.release();
        result
    }

    /// Register one handler (newest).
    fn register(&self, func: usize) -> Result<(), Errno> {
        self.with_table(|table| table.register(func))
    }

    /// Take the newest handler.
    fn next(&self) -> Option<Entry> {
        self.with_table(QuickExitTable::pop_next)
    }
}

/// The one process registry.
static REGISTRY: Registry = Registry::new();

/// Call one consumed handler.
fn call(entry: Entry) {
    if entry.func == 0 {
        return;
    }
    // SAFETY: only `at_quick_exit` stores this address, and it registered an `extern "C" fn()`.
    let handler: extern "C" fn() = unsafe { core::mem::transmute(entry.func) };
    handler();
}

/// `at_quick_exit(3)`: register a handler for `quick_exit`.
#[cfg(target_os = "none")]
#[unsafe(no_mangle)]
pub extern "C" fn at_quick_exit(handler: extern "C" fn()) -> c_int {
    match REGISTRY.register(handler as usize) {
        Ok(()) => 0,
        Err(_) => 1,
    }
}

/// `quick_exit(3)`: run the handlers newest first, then terminate without `atexit` or stdio.
#[cfg(target_os = "none")]
#[unsafe(no_mangle)]
pub extern "C" fn quick_exit(status: c_int) -> ! {
    while let Some(entry) = REGISTRY.next() {
        call(entry);
    }
    crate::unistd::_exit::_exit(status)
}
