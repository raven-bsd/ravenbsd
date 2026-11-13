// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! A safe `MockKernel` for the unit tests: a real heap TCB, an atomic fake clock and recorded
//! kernel calls. No mapping, no second thread and no unsafe.

use core::sync::atomic::{AtomicI32, AtomicU64, AtomicUsize, Ordering};

use raven_libc_tls::Pthread;

use crate::kernel::{CreateArgs, KernelCaps, ThreadKernel};
use crate::limits::{ENOMEM, Errno};

static SERIAL: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// Serialise tests that touch the process-wide thread state.
pub fn serial() -> std::sync::MutexGuard<'static, ()> {
    match SERIAL.lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    }
}

/// The unit-test kernel.
pub struct MockKernel {
    caps: KernelCaps,
    tcb: AtomicUsize,
    now: AtomicU64,
    wait_result: AtomicI32,
    wait_calls: AtomicUsize,
    wake_calls: AtomicUsize,
    kick_calls: AtomicUsize,
    last_timeout: AtomicU64,
    points: AtomicUsize,
}

impl MockKernel {
    /// A full-capability kernel with a fresh TCB in thread state.
    #[must_use]
    pub fn new() -> Self {
        Self::with_caps(KernelCaps::full())
    }

    /// A kernel with the given capabilities.
    #[must_use]
    pub fn with_caps(caps: KernelCaps) -> Self {
        let mut boxed = alloc::boxed::Box::new(Pthread::new(0));
        let address = &mut *boxed as *mut Pthread as usize;
        boxed.self_ptr = address;
        boxed.tid.store(100, Ordering::Release);
        boxed.robust_head = address + core::mem::offset_of!(Pthread, robust_head);
        boxed.tsd = crate::thread::tsd::main_tsd_address();
        boxed
            .detach_state
            .store(raven_libc_tls::tcb::DT_JOINABLE, Ordering::Release);
        boxed.cancel.store(1, Ordering::Release);
        boxed.prev = &mut *boxed as *mut Pthread;
        boxed.next = &mut *boxed as *mut Pthread;
        let leaked: &'static mut Pthread = alloc::boxed::Box::leak(boxed);
        crate::raw::install_exit_hook(|code| std::panic::panic_any(code));
        Self {
            caps,
            tcb: AtomicUsize::new(leaked as *mut Pthread as usize),
            now: AtomicU64::new(1_000_000),
            wait_result: AtomicI32::new(0),
            wait_calls: AtomicUsize::new(0),
            wake_calls: AtomicUsize::new(0),
            kick_calls: AtomicUsize::new(0),
            last_timeout: AtomicU64::new(u64::MAX),
            points: AtomicUsize::new(0),
        }
    }

    /// The TCB address.
    #[must_use]
    pub fn tcb(&self) -> usize {
        self.tcb.load(Ordering::Acquire)
    }

    /// Set the virtual clock.
    pub fn set_now(&self, value: u64) {
        self.now.store(value, Ordering::Release);
    }

    /// Make the next futex wait fail with `error` (0 restores success).
    pub fn set_wait_result(&self, error: i32) {
        self.wait_result.store(error, Ordering::Release);
    }

    /// The number of futex waits.
    #[must_use]
    pub fn wait_calls(&self) -> usize {
        self.wait_calls.load(Ordering::Acquire)
    }

    /// The last relative timeout given to a wait.
    #[must_use]
    pub fn last_timeout(&self) -> u64 {
        self.last_timeout.load(Ordering::Acquire)
    }

    /// The number of futex wakes.
    #[must_use]
    pub fn wake_calls(&self) -> usize {
        self.wake_calls.load(Ordering::Acquire)
    }

    /// The number of kicks.
    #[must_use]
    pub fn kick_calls(&self) -> usize {
        self.kick_calls.load(Ordering::Acquire)
    }

    /// The number of scheduler points.
    #[must_use]
    pub fn points(&self) -> usize {
        self.points.load(Ordering::Acquire)
    }
}

impl Default for MockKernel {
    fn default() -> Self {
        Self::new()
    }
}

impl ThreadKernel for MockKernel {
    fn caps(&self) -> KernelCaps {
        self.caps
    }

    fn thread_pointer(&self) -> usize {
        raven_libc_tls::tcb::pthread_to_tp(self.tcb())
    }

    fn set_thread_pointer(&self, tp: usize) -> Result<(), Errno> {
        self.tcb
            .store(raven_libc_tls::tcb::tp_to_pthread(tp), Ordering::Release);
        Ok(())
    }

    fn tid_self(&self) -> Result<i32, Errno> {
        Ok(100)
    }

    fn create(&self, _args: &CreateArgs) -> Result<u32, Errno> {
        Err(ENOMEM)
    }

    fn exit(&self, _code: i32) -> ! {
        panic!("mock exit")
    }

    fn futex_wait(
        &self,
        _w: &core::sync::atomic::AtomicU32,
        _expected: u32,
        rel_ns: Option<u64>,
    ) -> Result<(), Errno> {
        self.wait_calls.fetch_add(1, Ordering::AcqRel);
        self.last_timeout
            .store(rel_ns.unwrap_or(u64::MAX), Ordering::Release);
        let error = self.wait_result.load(Ordering::Acquire);
        if error == 0 {
            Ok(())
        } else {
            Err(Errno(error as u16))
        }
    }

    fn futex_wake(&self, _w: &core::sync::atomic::AtomicU32, _n: u32) -> u32 {
        self.wake_calls.fetch_add(1, Ordering::AcqRel);
        1
    }

    fn futex_requeue(
        &self,
        _w: &core::sync::atomic::AtomicU32,
        _wake: u32,
        _to: &core::sync::atomic::AtomicU32,
        max: u32,
    ) -> Result<u32, Errno> {
        Ok(max.min(1))
    }

    fn map(&self, _len: usize) -> Result<usize, Errno> {
        Err(ENOMEM)
    }

    fn unmap(&self, _addr: usize, _len: usize) -> Result<(), Errno> {
        Ok(())
    }

    fn protect_none(&self, _addr: usize, _len: usize) -> Result<(), Errno> {
        Ok(())
    }

    fn kick(&self, _handle: u32, _bits: u64) -> Result<(), Errno> {
        self.kick_calls.fetch_add(1, Ordering::AcqRel);
        Ok(())
    }

    fn yield_now(&self) {}

    fn now_ns(&self, _clock: u32) -> u64 {
        self.now.load(Ordering::Acquire)
    }

    fn set_robust_list(&self, _head: usize) -> Result<(), Errno> {
        Ok(())
    }

    fn fork_process(&self) -> Result<i32, Errno> {
        Ok(0)
    }

    fn start_trampoline(&self) -> fn(usize) -> ! {
        |_| panic!("mock trampoline")
    }

    fn sched_point(&self, _name: &'static str) {
        self.points.fetch_add(1, Ordering::AcqRel);
    }
}
