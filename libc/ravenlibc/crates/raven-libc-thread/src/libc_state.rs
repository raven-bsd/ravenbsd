// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The process-wide thread state: one `static`, atomics and fixed arrays only.
//!
//! `need_locks` follows musl's `__lock.c` protocol: 0 means the process is single-threaded and
//! internal locks are skipped, 1 means the process is multi-threaded, and -1 is the "last thread
//! just exited" marker that the next `__lock` turns back into 0.

use core::sync::atomic::{AtomicI32, AtomicUsize, Ordering};

use crate::limits::{DEFAULT_GUARD_SIZE, DEFAULT_STACK_SIZE};

/// The global thread state.
pub struct LibcThreads {
    /// Thread count including the main thread.
    threads: AtomicUsize,
    /// `0` single-threaded, `1` multi-threaded, `-1` last exit not yet observed.
    need_locks: AtomicI32,
    /// User-space tid counter when PG-2 is absent.
    next_tid: AtomicI32,
    /// TCB address of the main thread.
    main_tcb: AtomicUsize,
    /// Process exit hook installed by the shell or a test.
    exit_hook: AtomicUsize,
    /// Default stack size.
    default_stack: AtomicUsize,
    /// Default guard size.
    default_guard: AtomicUsize,
}

impl LibcThreads {
    /// The initial single-threaded state.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            threads: AtomicUsize::new(1),
            need_locks: AtomicI32::new(0),
            next_tid: AtomicI32::new(100),
            main_tcb: AtomicUsize::new(0),
            exit_hook: AtomicUsize::new(0),
            default_stack: AtomicUsize::new(DEFAULT_STACK_SIZE),
            default_guard: AtomicUsize::new(DEFAULT_GUARD_SIZE),
        }
    }

    /// Number of live threads.
    #[must_use]
    pub fn threads(&self) -> usize {
        self.threads.load(Ordering::Acquire)
    }

    /// Add one thread; the first addition flips `need_locks` to 1.
    pub fn add_thread(&self) -> usize {
        let previous = self.threads.fetch_add(1, Ordering::AcqRel);
        if previous <= 1 {
            self.need_locks.store(1, Ordering::Release);
        }
        previous.saturating_add(1)
    }

    /// Remove one thread; the last removal marks `need_locks` for the next lock.
    pub fn remove_thread(&self) -> usize {
        let previous = self.threads.fetch_sub(1, Ordering::AcqRel);
        if previous <= 1 {
            self.need_locks.store(-1, Ordering::Release);
            0
        } else {
            previous.saturating_sub(1)
        }
    }

    /// Reset to a single main thread (startup and each serialised host test).
    pub fn reset(&self, main_tcb: usize) {
        self.threads.store(1, Ordering::Release);
        self.need_locks.store(0, Ordering::Release);
        self.next_tid.store(100, Ordering::Release);
        self.main_tcb.store(main_tcb, Ordering::Release);
    }

    /// The main thread's TCB.
    #[must_use]
    pub fn main_tcb(&self) -> usize {
        self.main_tcb.load(Ordering::Acquire)
    }

    /// `libc.need_locks`.
    #[must_use]
    pub fn need_locks(&self) -> i32 {
        self.need_locks.load(Ordering::Acquire)
    }

    /// Set the multi-threaded flag (the fork child clears it).
    pub fn set_need_locks(&self, value: i32) {
        self.need_locks.store(value, Ordering::Release);
    }

    /// The next user-space tid: 30 bits, never zero.
    pub fn next_tid(&self) -> i32 {
        loop {
            let value = self.next_tid.fetch_add(1, Ordering::AcqRel);
            if value & 0x3fff_ffff != 0 {
                let masked = value & 0x3fff_ffff;
                if masked != 0 && masked != 0x3fff_ffff {
                    return masked;
                }
            }
            self.next_tid.store(100, Ordering::Release);
        }
    }

    /// The default stack size.
    #[must_use]
    pub fn default_stack(&self) -> usize {
        self.default_stack.load(Ordering::Acquire)
    }

    /// The default guard size.
    #[must_use]
    pub fn default_guard(&self) -> usize {
        self.default_guard.load(Ordering::Acquire)
    }

    /// Raise the defaults (`pthread_setattr_default_np`).
    pub fn set_defaults(&self, stack: usize, guard: usize) {
        self.default_stack.fetch_max(stack, Ordering::AcqRel);
        self.default_guard.fetch_max(guard, Ordering::AcqRel);
    }

    /// The raw exit-hook slot (a `fn(i32) -> !` pointer).
    #[must_use]
    pub fn exit_hook(&self) -> usize {
        self.exit_hook.load(Ordering::Acquire)
    }

    /// Install the process exit hook.
    pub fn set_exit_hook(&self, hook: usize) {
        self.exit_hook.store(hook, Ordering::Release);
    }
}

impl Default for LibcThreads {
    fn default() -> Self {
        Self::new()
    }
}

/// The one process-wide state object.
pub static LIBC: LibcThreads = LibcThreads::new();

/// The process exit hook defaults to an abort loop; the shell installs the real `exit`.
pub(crate) fn exit_hook_default(_code: i32) -> ! {
    loop {
        core::hint::spin_loop();
    }
}

#[cfg(test)]
#[path = "libc_state_ut.rs"]
mod tests;

impl LibcThreads {
    /// Force the thread count (the fork child).
    pub fn set_threads(&self, count: usize) {
        self.threads.store(count, Ordering::Release);
    }
}
