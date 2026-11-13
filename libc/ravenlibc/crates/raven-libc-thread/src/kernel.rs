// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The kernel seam (`09-threads-tls-signals.md` 1.6, RD-new 2).
//!
//! `raven-libc-thread` defines `ThreadKernel`; `raven-libc-sys` cannot name thread types, so the
//! production implementation [`SysKernel`](crate::raw::SysKernel) lives in this crate's raw
//! module and calls C54's wrappers. The host fake implements the same trait with `std`.
//!
//! Every method the kernel does not provide yet has a documented fallback in the capability
//! matrix (C57 4.16): the caller checks [`KernelCaps`] and never sets a kernel flag without it.

use core::sync::atomic::AtomicU32;

use crate::limits::Errno;

/// What the current kernel can do. The host fake sets every flag from its constructor argument,
/// so every "until then" path is executable in a test.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct KernelCaps {
    /// PG-1: a settable thread pointer.
    pub tls: bool,
    /// PG-2: `THREAD_SELF`/real tids.
    pub tid: bool,
    /// PG-2: `THREAD_EXIT_EX`.
    pub exit_ex: bool,
    /// PG-3: `FUTEX_REQUEUE`.
    pub requeue: bool,
    /// PG-3: absolute futex deadlines.
    pub abs_deadline: bool,
    /// PG-4: shared futex keys.
    pub shared_futex: bool,
    /// PG-5: the kernel robust list.
    pub robust: bool,
    /// PG-6: priority-inheriting futexes.
    pub pi: bool,
    /// PG-7: `THREAD_KICK`.
    pub kick: bool,
    /// PG-8: `-EINTR` from a futex wait.
    pub eintr: bool,
    /// PG-11: scheduling parameters.
    pub sched: bool,
    /// PG-45: `PROC_FORK`.
    pub proc_fork: bool,
}

impl KernelCaps {
    /// A kernel with every Raven primitive proposed so far.
    #[must_use]
    pub const fn full() -> Self {
        Self {
            tls: true,
            tid: true,
            exit_ex: true,
            requeue: true,
            abs_deadline: true,
            shared_futex: true,
            robust: true,
            pi: true,
            kick: true,
            eintr: true,
            sched: true,
            proc_fork: true,
        }
    }

    /// A kernel with every optional primitive absent (today's on-target state).
    #[must_use]
    pub const fn minimal() -> Self {
        Self {
            tls: false,
            tid: false,
            exit_ex: false,
            requeue: false,
            abs_deadline: false,
            shared_futex: false,
            robust: false,
            pi: false,
            kick: false,
            eintr: false,
            sched: false,
            proc_fork: false,
        }
    }

    /// A copy with one flag changed; handy for the capability matrix.
    #[must_use]
    pub const fn with(mut self, flag: CapFlag, value: bool) -> Self {
        match flag {
            CapFlag::Tls => self.tls = value,
            CapFlag::Tid => self.tid = value,
            CapFlag::ExitEx => self.exit_ex = value,
            CapFlag::Requeue => self.requeue = value,
            CapFlag::AbsDeadline => self.abs_deadline = value,
            CapFlag::SharedFutex => self.shared_futex = value,
            CapFlag::Robust => self.robust = value,
            CapFlag::Pi => self.pi = value,
            CapFlag::Kick => self.kick = value,
            CapFlag::Eintr => self.eintr = value,
            CapFlag::Sched => self.sched = value,
            CapFlag::ProcFork => self.proc_fork = value,
        }
        self
    }

    /// The flag named by `flag`.
    #[must_use]
    pub const fn get(self, flag: CapFlag) -> bool {
        match flag {
            CapFlag::Tls => self.tls,
            CapFlag::Tid => self.tid,
            CapFlag::ExitEx => self.exit_ex,
            CapFlag::Requeue => self.requeue,
            CapFlag::AbsDeadline => self.abs_deadline,
            CapFlag::SharedFutex => self.shared_futex,
            CapFlag::Robust => self.robust,
            CapFlag::Pi => self.pi,
            CapFlag::Kick => self.kick,
            CapFlag::Eintr => self.eintr,
            CapFlag::Sched => self.sched,
            CapFlag::ProcFork => self.proc_fork,
        }
    }
}

/// One capability, for the matrix tests.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CapFlag {
    /// PG-1.
    Tls,
    /// PG-2.
    Tid,
    /// PG-2.
    ExitEx,
    /// PG-3.
    Requeue,
    /// PG-3.
    AbsDeadline,
    /// PG-4.
    SharedFutex,
    /// PG-5.
    Robust,
    /// PG-6.
    Pi,
    /// PG-7.
    Kick,
    /// PG-8.
    Eintr,
    /// PG-11.
    Sched,
    /// PG-45.
    ProcFork,
}

/// Every capability in the matrix order.
pub const ALL_CAPS: [CapFlag; 12] = [
    CapFlag::Tls,
    CapFlag::Tid,
    CapFlag::ExitEx,
    CapFlag::Requeue,
    CapFlag::AbsDeadline,
    CapFlag::SharedFutex,
    CapFlag::Robust,
    CapFlag::Pi,
    CapFlag::Kick,
    CapFlag::Eintr,
    CapFlag::Sched,
    CapFlag::ProcFork,
];

/// The arguments of one `thread_create`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CreateArgs {
    /// Entry stub (`__rl_thread_entry`) or the fake's trampoline marker.
    pub entry: usize,
    /// Top of the new stack (16-byte aligned by the caller).
    pub stack_top: usize,
    /// The `start_args` structure carved below `stack_top`.
    pub arg: usize,
    /// `TP_ADJ(new)`; the kernel installs it as the new thread's thread pointer (PG-1).
    pub tp: usize,
    /// Address of `Pthread.tid`; the kernel clears it on exit (PG-2).
    pub clear_tid_addr: usize,
}

/// The kernel primitives of the thread library.
///
/// Production has exactly one implementation ([`SysKernel`](crate::raw::SysKernel)); the host fake
/// has exactly one (`HostKernel`). A generic parameter keeps both monomorphised and avoids `dyn`.
pub trait ThreadKernel: Sync {
    /// What this kernel provides.
    fn caps(&self) -> KernelCaps;

    /// Read the calling thread's thread pointer.
    fn thread_pointer(&self) -> usize;

    /// Install a thread pointer for the calling thread (PG-1).
    fn set_thread_pointer(&self, tp: usize) -> Result<(), Errno>;

    /// The calling thread's kernel tid (PG-2); `ENOSYS` means "use the user counter".
    fn tid_self(&self) -> Result<i32, Errno>;

    /// Start a new thread, returning its handle.
    fn create(&self, args: &CreateArgs) -> Result<u32, Errno>;

    /// Terminate the calling thread.
    fn exit(&self, code: i32) -> !;

    /// Clear `clear_addr`, unmap `unmap_addr..unmap_addr+unmap_len` and exit as one kernel step
    /// (PG-2). Without PG-2 the library never calls this.
    fn exit_ex(&self, code: i32, clear_addr: usize, unmap_addr: usize, unmap_len: usize) -> ! {
        let _ = (clear_addr, unmap_addr, unmap_len);
        self.exit(code)
    }

    /// Wait on `w` while it equals `expected`. `rel_ns` is relative nanoseconds; `None` is
    /// forever and `Some(0)` is a caller bug (A-thr-5).
    fn futex_wait(&self, w: &AtomicU32, expected: u32, rel_ns: Option<u64>) -> Result<(), Errno>;

    /// Wake at most `n` waiters on `w`.
    fn futex_wake(&self, w: &AtomicU32, n: u32) -> u32;

    /// Move up to `max` waiters from `w` to `to`, waking `wake` first (PG-3). Without PG-3 this
    /// returns `ENOSYS` and the caller wakes one waiter.
    fn futex_requeue(
        &self,
        w: &AtomicU32,
        wake: u32,
        to: &AtomicU32,
        max: u32,
    ) -> Result<u32, Errno>;

    /// Map `len` zeroed writable bytes and return the address.
    fn map(&self, len: usize) -> Result<usize, Errno>;

    /// Unmap a range.
    fn unmap(&self, addr: usize, len: usize) -> Result<(), Errno>;

    /// Remove access from a range (the guard page, PG-13 fallback in the library).
    fn protect_none(&self, addr: usize, len: usize) -> Result<(), Errno>;

    /// Send signal bits to a thread handle (PG-7).
    fn kick(&self, handle: u32, bits: u64) -> Result<(), Errno>;

    /// Yield the processor.
    fn yield_now(&self);

    /// Read a clock in nanoseconds.
    fn now_ns(&self, clock: u32) -> u64;

    /// Register the kernel robust list (PG-5).
    fn set_robust_list(&self, head: usize) -> Result<(), Errno>;

    /// Whether the kernel has finished a thread's last user instruction (the clear-on-exit
    /// word in the production kernel). The host fake tracks the scheduler state; the default
    /// trusts the caller because the production `join` still waits on the thread-list lock.
    fn thread_is_dead(&self, _tcb: usize) -> bool {
        true
    }

    /// Fork the calling process (PG-45). The default is `ENOSYS`; the host fake returns a
    /// simulated child pid and the library performs the child-side repair in the same process.
    fn fork_process(&self) -> Result<i32, Errno> {
        Err(Errno(raven_libc_abi::errno::ENOSYS as u16))
    }

    /// The function the assembly entry stub calls with the `StartArgs` address.
    ///
    /// The production kernel returns a trampoline over `SysKernel`; the host fake returns its
    /// own trampoline and ignores the value in `CreateArgs.entry`.
    fn start_trampoline(&self) -> fn(usize) -> !;

    /// Deterministic-scheduler hand-over point; a no-op for the production kernel.
    fn sched_point(&self, _name: &'static str) {}
}

#[cfg(test)]
#[path = "kernel_ut.rs"]
mod tests;
