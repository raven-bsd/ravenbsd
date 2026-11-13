// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//!
//! The one `Platform` instance of the process and the shell's lock.
//!
//! `raven-libc-platform` is a library: the shell publishes the process-wide object (C55 module
//! documentation). The instance is initialised once on first use; until C57 lands, every libc
//! call is single-threaded (the kernel starts one thread per process), so the `RefCell` borrows
//! inside `Platform` cannot overlap. `ShellLock` is already the real seam for the fd table, so
//! the only assumption left is "one thread until C57".

use core::cell::UnsafeCell;
use core::mem::MaybeUninit;
use core::sync::atomic::{AtomicBool, AtomicU8, Ordering};

use raven_libc_platform::{Lock, Platform, RealSys};

/// The shell's process lock: one spin word.
#[derive(Debug, Default)]
pub(crate) struct ShellLock {
    held: AtomicBool,
}

impl Lock for ShellLock {
    fn acquire(&self) {
        while self.held.swap(true, Ordering::Acquire) {
            core::hint::spin_loop();
        }
    }

    fn release(&self) {
        self.held.store(false, Ordering::Release);
    }
}

const UNINIT: u8 = 0;
const READY: u8 = 2;

struct Global(UnsafeCell<MaybeUninit<Platform<RealSys, ShellLock>>>);

// SAFETY: the object is written exactly once before `READY` is published with Release, and a
// reader only obtains `&Platform` after an Acquire load of `READY`. The interior `RefCell`s are
// used by one thread at a time until C57 supplies the thread support (module documentation).
unsafe impl Sync for Global {}

static STATE: AtomicU8 = AtomicU8::new(UNINIT);
static PLATFORM: Global = Global(UnsafeCell::new(MaybeUninit::uninit()));

/// The process-wide platform.
pub(crate) fn global() -> &'static Platform<RealSys, ShellLock> {
    if STATE
        .compare_exchange(UNINIT, 1, Ordering::AcqRel, Ordering::Acquire)
        .is_ok()
    {
        // SAFETY: this caller won the one-shot initialisation and owns the slot until READY.
        unsafe {
            let slot = &mut *PLATFORM.0.get();
            slot.write(Platform::with_lock(RealSys, ShellLock::default()));
        }
        STATE.store(READY, Ordering::Release);
    }
    while STATE.load(Ordering::Acquire) != READY {
        core::hint::spin_loop();
    }
    // SAFETY: READY was published with Release after the write, and the value is never moved
    // again (`static` storage).
    unsafe { (&*PLATFORM.0.get()).assume_init_ref() }
}
