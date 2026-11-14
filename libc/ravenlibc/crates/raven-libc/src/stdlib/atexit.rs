// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//!
//! The process exit-handler registry: the shell instance of C41's [`ExitTable`].
//!
//! `donor-map/16` 3.A.4.3 and C61 4.3 item 9 keep the table in `raven-libc-stdlib`; this file
//! owns only the process-global instance, the tiny spin lock around it and the three C entry
//! points. The table semantics (the inline 32, the spill, consume-before-return, DSO matching)
//! are C41's and are not repeated here.
//!
//! No handler ever runs while the lock is held: [`ExitTable::next_to_run`] marks the entry
//! consumed under the lock and the handler is called after the release, so a handler may call
//! `atexit`/`__cxa_atexit` again (or `exit`, which would find the remaining entries).
//!
//! The module is declared from `lib.rs` (not `stdlib/mod.rs`) because `stdlib` is target-only
//! while the host unit tests of the registry must build; the C exports are target-only.

use core::cell::UnsafeCell;
use core::ffi::{c_int, c_void};

use raven_libc_abi::errno::Errno;
use raven_libc_stdlib::exit::{Entry, ExitTable, Kind};

use crate::ffi::lock::SpinLock;

/// The table and its lock.
struct Registry {
    /// Serialises every access to `table`.
    lock: SpinLock,
    /// C41's table; only touched while `lock` is held.
    table: UnsafeCell<ExitTable>,
}

// SAFETY: `lock` serialises every access to the `UnsafeCell`; `with_table` is the only reader
// or writer and no handler runs inside it.
unsafe impl Sync for Registry {}

impl Registry {
    /// An empty registry.
    const fn new() -> Self {
        Self {
            lock: SpinLock::new(),
            table: UnsafeCell::new(ExitTable::new()),
        }
    }

    /// Run `visit` on the table with the lock held.
    fn with_table<R>(&self, visit: impl FnOnce(&mut ExitTable) -> R) -> R {
        self.lock.acquire();
        // SAFETY: the lock is held for the whole visit and released before any handler runs.
        let result = visit(unsafe { &mut *self.table.get() });
        self.lock.release();
        result
    }

    /// Register one handler (newest).
    fn register(&self, entry: Entry) -> Result<(), Errno> {
        self.with_table(|table| table.register(entry))
    }

    /// Run every unconsumed handler, newest first.
    fn run_all(&self) {
        while let Some(entry) = self.with_table(ExitTable::next_to_run) {
            call(entry);
        }
    }

    /// Run the handlers registered for `dso`; a NULL handle runs every handler.
    fn finalize(&self, dso: *mut c_void) {
        if dso.is_null() {
            self.run_all();
            return;
        }
        let handle = dso as usize;
        while let Some(entry) = self.with_table(|table| table.finalize_dso(handle)) {
            call(entry);
        }
    }

    /// The number of unconsumed handlers.
    fn pending(&self) -> usize {
        self.with_table(|table| table.pending())
    }
}

/// The one process registry.
static REGISTRY: Registry = Registry::new();

/// Run every exit handler, newest first (`exit`, `__rl_fini_array`'s caller).
#[allow(dead_code)] // called by the target-only strong hook and the unit tests
pub(crate) fn run_all() {
    REGISTRY.run_all();
}

/// Run the handlers of `dso`; NULL runs all (`__cxa_finalize`).
#[allow(dead_code)] // the target-only C export and the unit tests are the users
pub(crate) fn finalize(dso: *mut c_void) {
    REGISTRY.finalize(dso);
}

/// The number of unconsumed handlers.
#[allow(dead_code)] // introspection for the unit tests and the `_SC_ATEXIT_MAX` follow-up
pub(crate) fn pending() -> usize {
    REGISTRY.pending()
}

/// One `atexit` entry: no argument and no DSO handle.
fn atexit_entry(handler: extern "C" fn()) -> Entry {
    Entry {
        func: handler as usize,
        arg: 0,
        dso: 0,
        kind: Kind::Atexit,
    }
}

/// One `__cxa_atexit` entry; all three words are opaque to this layer.
fn cxa_entry(
    handler: Option<extern "C" fn(*mut c_void)>,
    arg: *mut c_void,
    dso: *mut c_void,
) -> Entry {
    Entry {
        func: handler.map_or(0, |function| function as usize),
        arg: arg as usize,
        dso: dso as usize,
        kind: Kind::Cxa,
    }
}

/// Call one consumed entry with the signature its registration kind implies.
fn call(entry: Entry) {
    if entry.func == 0 {
        return;
    }
    match entry.kind {
        Kind::Atexit => {
            // SAFETY: only `atexit` stores this address and it registered an `extern "C" fn()`.
            let handler: extern "C" fn() = unsafe { core::mem::transmute(entry.func) };
            handler();
        }
        Kind::Cxa => {
            // SAFETY: only `__cxa_atexit` stores this address and it registered an
            // `extern "C" fn(*mut c_void)`; `arg` is the word supplied at the same time.
            let handler: extern "C" fn(*mut c_void) = unsafe { core::mem::transmute(entry.func) };
            handler(entry.arg as *mut c_void);
        }
    }
}

/// Register one `atexit` handler in the process registry.
#[allow(dead_code)] // the target export and the unit tests drive the registry through `Registry`
pub(crate) fn register_atexit(handler: extern "C" fn()) -> Result<(), Errno> {
    REGISTRY.register(atexit_entry(handler))
}

/// Register one `__cxa_atexit` handler in the process registry.
#[allow(dead_code)] // the target export/start and the unit tests drive `Registry` directly
pub(crate) fn register_cxa(
    handler: Option<extern "C" fn(*mut c_void)>,
    arg: *mut c_void,
    dso: *mut c_void,
) -> Result<(), Errno> {
    REGISTRY.register(cxa_entry(handler, arg, dso))
}

/// Map a table result to the C convention: 0 on success, -1 with `ENOMEM` on failure.
fn status(result: Result<(), Errno>) -> c_int {
    crate::ffi::ret::unit(result)
}

/// `atexit(3)`: register a handler with no argument.
#[cfg(target_os = "none")]
#[unsafe(no_mangle)]
pub extern "C" fn atexit(handler: extern "C" fn()) -> c_int {
    status(register_atexit(handler))
}

/// The C++ ABI registration routine (`__cxa_atexit`).
#[cfg(target_os = "none")]
#[unsafe(no_mangle)]
pub extern "C" fn __cxa_atexit(
    handler: Option<extern "C" fn(*mut c_void)>,
    arg: *mut c_void,
    dso: *mut c_void,
) -> c_int {
    status(register_cxa(handler, arg, dso))
}

/// The C++ ABI finaliser (`__cxa_finalize`); a NULL handle runs every handler.
#[cfg(target_os = "none")]
#[unsafe(no_mangle)]
pub extern "C" fn __cxa_finalize(dso: *mut c_void) {
    REGISTRY.finalize(dso);
}

#[cfg(test)]
#[path = "atexit_ut.rs"]
mod tests;
