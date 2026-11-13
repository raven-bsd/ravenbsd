// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//!
//! The allocator instantiation: `raven-libc-malloc`'s tiny backend over the syscall page
//! provider, a spin lock and the shell's `errno` cell.
//!
//! This is seam glue, not allocator logic: every rule lives in `raven-libc-malloc` (C49). The
//! hardened backend and the debug wrapper wait for the thread/tls crates (C57) and for
//! `__raven_libc_init` (C61); the tiny backend is the `tiny` profile of the hook table.

use core::sync::atomic::{AtomicBool, Ordering};

use raven_libc_malloc::compat::{Mallinfo, Mallinfo2};
use raven_libc_malloc::{Api, Lock, PageProvider, Platform, Tiny};
use raven_libc_sys::pages::SysPageProvider;

/// The non-allocating page provider over `MAP`/`UNMAP`/`PROTECT`.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct Pages;

impl PageProvider for Pages {
    fn page_size(&self) -> usize {
        SysPageProvider::new().page_size()
    }

    fn map_pages(&self, len: usize, align: usize, prot: u8) -> Option<usize> {
        SysPageProvider::new().map_pages(len, align, prot)
    }

    fn map_exact(&self, hint: usize, len: usize) -> Option<usize> {
        SysPageProvider::new().map_exact(hint, len)
    }

    fn unmap(&self, addr: usize, len: usize) -> bool {
        SysPageProvider::new().unmap(addr, len)
    }

    fn protect(&self, addr: usize, len: usize, prot: u8) -> bool {
        SysPageProvider::new().protect(addr, len, prot)
    }

    fn seal(&self, addr: usize, len: usize) -> bool {
        SysPageProvider::new().seal(addr, len)
    }
}

/// The allocator's lock registry: one spin word per pool index.
#[derive(Debug, Default)]
pub(crate) struct Spin {
    words: [AtomicBool; 32],
}

impl Spin {
    /// A fresh registry.
    pub(crate) const fn new() -> Self {
        Self {
            words: [const { AtomicBool::new(false) }; 32],
        }
    }
}

impl Lock for Spin {
    fn lock(&self, index: usize) {
        let Some(word) = self.words.get(index) else {
            return;
        };
        while word.swap(true, Ordering::Acquire) {
            core::hint::spin_loop();
        }
    }

    fn unlock(&self, index: usize) {
        if let Some(word) = self.words.get(index) {
            word.store(false, Ordering::Release);
        }
    }

    fn reset(&self, index: usize) {
        self.unlock(index);
    }
}

/// The allocator's view of the process.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct Hooks;

impl Platform for Hooks {
    fn thread_id(&self) -> usize {
        1
    }

    fn set_errno(&self, e: i32) {
        crate::ffi::errno::set_raw(e);
    }

    fn get_errno(&self) -> i32 {
        crate::ffi::errno::get()
    }

    fn write_stderr(&self, b: &[u8]) {
        let message = b"libc: allocator: ";
        // SAFETY: `DEBUG_WRITE` reads both buffers for their stated lengths and returns a
        // scalar; the slices outlive the call.
        unsafe {
            let _ = raven_libc_sys::arch::syscall3(
                raven_abi_syscall::DEBUG_WRITE,
                2,
                message.as_ptr() as usize,
                message.len(),
            );
            let _ = raven_libc_sys::arch::syscall3(
                raven_abi_syscall::DEBUG_WRITE,
                2,
                b.as_ptr() as usize,
                b.len(),
            );
        }
    }

    fn abort(&self) -> ! {
        raven_libc_sys::sys::exit(134)
    }

    fn progname(&self) -> &[u8] {
        b"libc"
    }

    fn process_id(&self) -> u32 {
        1
    }

    fn mallinfo(&self) -> Mallinfo {
        Mallinfo::default()
    }

    fn mallinfo2(&self) -> Mallinfo2 {
        Mallinfo2::default()
    }
}

/// The `DEBUG_WRITE` syscall number without a `raven-abi` dependency in the shell.
mod raven_abi_syscall {
    /// `raven_abi::syscall::Syscall::DEBUG_WRITE`.
    pub(crate) const DEBUG_WRITE: usize = 0x03;
}

/// The one allocator of the process.
static HEAP: Api<Tiny<Pages, Spin, Hooks>, Hooks> =
    Api::new(Tiny::new(Pages, Spin::new(), Hooks), Hooks);

/// The allocator front end.
pub(crate) fn api() -> &'static Api<Tiny<Pages, Spin, Hooks>, Hooks> {
    &HEAP
}
