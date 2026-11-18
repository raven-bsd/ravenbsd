// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//!
//! The main-thread kernel seam used by `__raven_init_main_thread`.
//!
//! `init_main_thread` uses the static `builtin_tls` block whenever it fits; only an executable
//! whose TLS exceeds that block maps pages. Recording the page backend behind function-pointer
//! statics keeps the page syscalls out of the static start graph while the runtime large-TLS
//! path still maps exactly as before.

use core::sync::atomic::AtomicU32;

use raven_libc_abi::errno::Errno;
use raven_libc_thread::kernel::{CreateArgs, KernelCaps, ThreadKernel};
use raven_libc_thread::raw::SysKernel;

static MAP: fn(&SysKernel, usize) -> Result<usize, Errno> = SysKernel::map;
static UNMAP: fn(&SysKernel, usize, usize) -> Result<(), Errno> = SysKernel::unmap;

/// The main-thread kernel seam: delegates everything, but the page path is an indirect call.
pub(crate) struct StartupKernel {
    inner: SysKernel,
}

impl StartupKernel {
    pub(crate) fn new() -> Self {
        Self {
            inner: SysKernel::new(),
        }
    }
}

impl ThreadKernel for StartupKernel {
    fn caps(&self) -> KernelCaps {
        self.inner.caps()
    }

    fn thread_pointer(&self) -> usize {
        self.inner.thread_pointer()
    }

    fn set_thread_pointer(&self, tp: usize) -> Result<(), Errno> {
        self.inner.set_thread_pointer(tp)
    }

    fn tid_self(&self) -> Result<i32, Errno> {
        self.inner.tid_self()
    }

    fn create(&self, args: &CreateArgs) -> Result<u32, Errno> {
        self.inner.create(args)
    }

    fn exit(&self, code: i32) -> ! {
        self.inner.exit(code)
    }

    fn futex_wait(
        &self,
        word: &AtomicU32,
        expected: u32,
        rel_ns: Option<u64>,
    ) -> Result<(), Errno> {
        self.inner.futex_wait(word, expected, rel_ns)
    }

    fn futex_wake(&self, word: &AtomicU32, n: u32) -> u32 {
        self.inner.futex_wake(word, n)
    }

    fn futex_requeue(
        &self,
        word: &AtomicU32,
        wake: u32,
        to: &AtomicU32,
        max: u32,
    ) -> Result<u32, Errno> {
        self.inner.futex_requeue(word, wake, to, max)
    }

    fn map(&self, len: usize) -> Result<usize, Errno> {
        MAP(&self.inner, len)
    }

    fn unmap(&self, addr: usize, len: usize) -> Result<(), Errno> {
        UNMAP(&self.inner, addr, len)
    }

    fn protect_none(&self, addr: usize, len: usize) -> Result<(), Errno> {
        self.inner.protect_none(addr, len)
    }

    fn kick(&self, handle: u32, bits: u64) -> Result<(), Errno> {
        self.inner.kick(handle, bits)
    }

    fn yield_now(&self) {
        self.inner.yield_now();
    }

    fn now_ns(&self, clock: u32) -> u64 {
        self.inner.now_ns(clock)
    }

    fn set_robust_list(&self, head: usize) -> Result<(), Errno> {
        self.inner.set_robust_list(head)
    }

    fn start_trampoline(&self) -> fn(usize) -> ! {
        self.inner.start_trampoline()
    }
}
