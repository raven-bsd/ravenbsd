// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `HostSigKernel`: a simulated delivery-frame kernel for the host tests (ticket 4.4).
//!
//! It keeps one simulated thread with a real `Pthread` TCB (so the mask and private state live in
//! the same words production uses), a `FakeCtx { pc, sp, regs, mask }`, a separate kernel context
//! area (`mcontext_t`), a scripted process-pending queue, a fault injector, `-EINTR`/timeout
//! results, a trace and failure injection. `safe_point` is where a real kernel would check the
//! kick word on the return to user mode: it copies the fake context into the area, calls the Rust
//! body of `__raven_sigentry` and restores the fake context after `SIGRETURN` (the fake implements
//! `sigreturn` by unwinding a private payload). No real signal is ever raised.

use core::sync::atomic::Ordering;

use raven_libc_abi::errno::Errno;
use raven_libc_abi::structs::{McontextT, SiginfoT};
use raven_libc_tls::tcb::Pthread;

use crate::dispositions::DispTable;
use crate::entry::{EntryOutcome, signal_entry};
use crate::errors::{ENOSYS_ERR, ETIMEDOUT_ERR};
use crate::frame;
use crate::kernel::{FaultRecord, KICK_CHECK, SigCaps, SigKernel};
use crate::raw;

/// One lock for every test that drives the fake kernel: the crate-wide errno fallback and the
/// static hook cells are process-global until C57 installs the TCB provider.
pub static GLOBAL_TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// The fake kernel's process id.
pub const FAKE_PID: i32 = 4242;
/// The fake kernel's thread handle.
pub const FAKE_HANDLE: u32 = 7;
/// The fake uid.
pub const FAKE_UID: u32 = 1000;
/// The default simulated stack size.
pub const STACK_SIZE: usize = 64 * 1024;

/// One simulated thread context (the shape of the kernel-saved area, ticket 4.6).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct FakeCtx {
    /// Program counter.
    pub pc: usize,
    /// Stack pointer.
    pub sp: usize,
    /// General registers.
    pub regs: [usize; 32],
    /// Signal mask word.
    pub mask: u64,
}

impl FakeCtx {
    /// A context with the given pc/sp on a zeroed register file.
    #[must_use]
    pub fn new(pc: usize, sp: usize) -> Self {
        Self {
            pc,
            sp,
            regs: [0; 32],
            mask: 0,
        }
    }
}

/// One recorded action.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Trace {
    /// A kick was delivered.
    Kick(u32),
    /// The entry body ran.
    Entry(u32),
    /// A handler ran with this signal.
    Handler(i32),
    /// `SIGRETURN` restored the context.
    Sigreturn,
    /// `SIGRESEND`.
    Resend(i32),
    /// procd kill.
    ProcKill(i32, i32),
    /// The wait timed out or blocked.
    Wait(Option<u64>),
    /// A default action terminated the process.
    Terminate(i32, bool),
}

/// The result of one `safe_point`.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct SafePoint {
    /// What the entry did, when it returned through `SIGRETURN`.
    pub outcome: Option<EntryOutcome>,
    /// `(sig, core)` when a default action terminated the simulated process.
    pub terminated: Option<(i32, bool)>,
    /// The kick had nothing to deliver.
    pub spurious: bool,
}

struct ThreadState {
    tcb: Box<Pthread>,
    ctx: FakeCtx,
    area: Box<McontextT>,
    _stack: Box<[u8]>,
    kick: u32,
}

struct Inner {
    thread: ThreadState,
    pending: Vec<(i32, SiginfoT)>,
    trace: Vec<Trace>,
    caps: SigCaps,
    clock: u64,
    failures: Vec<&'static str>,
    waited: bool,
    proc_infos: Vec<(i32, i32, SiginfoT)>,
}

/// The host fake.
pub struct HostSigKernel {
    inner: std::sync::Mutex<Inner>,
}

impl HostSigKernel {
    /// A kernel with every capability on.
    #[must_use]
    pub fn new() -> Self {
        let mut tcb = Box::new(Pthread::new(0));
        let pointer = (&*tcb as *const Pthread) as usize;
        tcb.self_ptr = pointer;
        tcb.handle = FAKE_HANDLE;
        tcb.sigmask.store(0, Ordering::Release);
        let mut stack = vec![0u8; STACK_SIZE].into_boxed_slice();
        let sp = stack.as_mut_ptr() as usize + STACK_SIZE - 1024;
        let ctx = FakeCtx::new(0x1000, sp);
        let area = Box::new(McontextT::ZERO);
        raw::set_ctx_area(pointer, (&*area as *const McontextT) as usize);
        Self {
            inner: std::sync::Mutex::new(Inner {
                thread: ThreadState {
                    tcb,
                    ctx,
                    area,
                    _stack: stack,
                    kick: 0,
                },
                pending: Vec::new(),
                trace: Vec::new(),
                caps: SigCaps::HOST,
                clock: 1_000,
                failures: Vec::new(),
                waited: false,
                proc_infos: Vec::new(),
            }),
        }
    }

    /// A kernel with an explicit capability set.
    #[must_use]
    pub fn with_caps(caps: SigCaps) -> Self {
        let kernel = Self::new();
        {
            let mut inner = kernel.inner.lock().unwrap();
            inner.caps = caps;
        }
        kernel
    }

    /// Remove the process-directed capability (the `no_proc_kill` scenario).
    pub fn disable_proc_kill(&self) {
        let mut inner = self.inner.lock().unwrap();
        inner.caps.proc_kill = false;
    }

    /// The simulated thread pointer.
    #[must_use]
    pub fn pointer(&self) -> usize {
        let inner = self.inner.lock().unwrap();
        (&*inner.thread.tcb as *const Pthread) as usize
    }

    /// The thread context.
    #[must_use]
    pub fn context(&self) -> FakeCtx {
        self.inner.lock().unwrap().thread.ctx
    }

    /// Replace the thread context (the test's "interrupted code").
    pub fn set_context(&self, context: FakeCtx) {
        let mut inner = self.inner.lock().unwrap();
        let tp = (&*inner.thread.tcb as *const Pthread) as usize;
        raw::mask_store(tp, context.mask);
        inner.thread.ctx = context;
    }

    /// The current mask word.
    #[must_use]
    pub fn mask(&self) -> u64 {
        let inner = self.inner.lock().unwrap();
        let tp = (&*inner.thread.tcb as *const Pthread) as usize;
        raw::mask_load(tp)
    }

    /// The recorded trace.
    #[must_use]
    pub fn trace(&self) -> Vec<Trace> {
        self.inner.lock().unwrap().trace.clone()
    }

    /// Clear the trace.
    pub fn clear_trace(&self) {
        self.inner.lock().unwrap().trace.clear();
    }

    /// Whether a handler ever ran.
    #[must_use]
    pub fn handler_count(&self) -> usize {
        self.trace()
            .iter()
            .filter(|entry| matches!(entry, Trace::Handler(_)))
            .count()
    }

    /// Simulate a kernel kick.
    pub fn kick_self(&self, bits: u32) {
        let mut inner = self.inner.lock().unwrap();
        inner.thread.kick |= bits;
        inner.trace.push(Trace::Kick(bits));
    }

    /// The sticky kick word.
    #[must_use]
    pub fn kick_word(&self) -> u32 {
        self.inner.lock().unwrap().thread.kick
    }

    /// The last `(pid, sig, info)` forwarded to procd.
    #[must_use]
    pub fn last_proc_info(&self) -> Option<(i32, i32, SiginfoT)> {
        self.inner.lock().unwrap().proc_infos.last().copied()
    }

    /// Queue a process-directed record.
    pub fn push_pending(&self, sig: i32, info: SiginfoT) {
        self.inner.lock().unwrap().pending.push((sig, info));
    }

    /// Simulate the return-to-user safe point: process the kick word.
    pub fn safe_point(
        &self,
        table: &DispTable,
        bits_override: Option<u32>,
        fault: Option<FaultRecord>,
    ) -> SafePoint {
        let (bits, tp) = {
            let mut inner = self.inner.lock().unwrap();
            let bits = bits_override.unwrap_or(inner.thread.kick);
            if bits == 0 {
                return SafePoint {
                    outcome: None,
                    terminated: None,
                    spurious: true,
                };
            }
            if bits_override.is_none() {
                inner.thread.kick = 0;
            }
            let tp = (&*inner.thread.tcb as *const Pthread) as usize;
            let ctx = inner.thread.ctx;
            let mut machine = McontextT::ZERO;
            for (index, value) in ctx.regs.iter().enumerate() {
                frame::set_ctx_greg(&mut machine, index, *value);
            }
            frame::set_ctx_pc(&mut machine, ctx.pc);
            frame::set_ctx_sp(&mut machine, ctx.sp);
            raw::store_ctx(tp, &machine);
            raw::mask_store(tp, ctx.mask);
            inner.trace.push(Trace::Entry(bits));
            (bits, tp)
        };
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            signal_entry(self, table, bits, fault);
        }));
        match result {
            Err(payload) => {
                if payload.downcast_ref::<SigReturn>().is_some() {
                    let mut inner = self.inner.lock().unwrap();
                    inner.trace.push(Trace::Sigreturn);
                    let machine = *inner.thread.area;
                    let mut ctx = inner.thread.ctx;
                    ctx.pc = frame::ctx_pc(&machine);
                    ctx.sp = frame::ctx_sp(&machine);
                    ctx.mask = raw::mask_load(tp);
                    for index in 0..32 {
                        ctx.regs[index] = frame::ctx_greg(&machine, index);
                    }
                    inner.thread.ctx = ctx;
                    SafePoint {
                        outcome: Some(EntryOutcome {
                            delivered: 0,
                            spurious: false,
                        }),
                        terminated: None,
                        spurious: false,
                    }
                } else if let Some(exit) = payload.downcast_ref::<Terminated>() {
                    let mut inner = self.inner.lock().unwrap();
                    inner.trace.push(Trace::Terminate(exit.sig, exit.core));
                    SafePoint {
                        outcome: None,
                        terminated: Some((exit.sig, exit.core)),
                        spurious: false,
                    }
                } else {
                    std::panic::resume_unwind(payload)
                }
            }
            Ok(never) => match never {},
        }
    }

    /// Run one entry directly (a `raise`-style delivery) without `SIGRETURN`.
    pub fn direct(
        &self,
        table: &DispTable,
        bits: u32,
        fault: Option<FaultRecord>,
    ) -> Result<EntryOutcome, (i32, bool)> {
        match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            crate::entry::run_direct(self, table, bits, fault)
        })) {
            Ok(outcome) => Ok(outcome),
            Err(payload) => {
                if let Some(exit) = payload.downcast_ref::<Terminated>() {
                    Err((exit.sig, exit.core))
                } else {
                    std::panic::resume_unwind(payload)
                }
            }
        }
    }

    /// The failure injection hook: any method listed here returns `EIO`.
    pub fn fail_next(&self, method: &'static str) {
        self.inner.lock().unwrap().failures.push(method);
    }

    fn failing(&self, method: &'static str) -> bool {
        let mut inner = self.inner.lock().unwrap();
        if let Some(index) = inner.failures.iter().position(|name| *name == method) {
            inner.failures.remove(index);
            return true;
        }
        false
    }

    fn record(&self, entry: Trace) {
        self.inner.lock().unwrap().trace.push(entry);
    }
}

// SAFETY: the fake is a single-threaded simulation guarded by its `Mutex`; the raw pointers in
// `Pthread` are never used by another thread here. The unsafe impl is test-only.
unsafe impl Send for Inner {}

impl Default for HostSigKernel {
    fn default() -> Self {
        Self::new()
    }
}

/// `sigreturn` unwinds with this payload.
pub struct SigReturn;

/// A default action terminated the simulated process.
pub struct Terminated {
    /// The signal.
    pub sig: i32,
    /// Whether the core flag is set.
    pub core: bool,
}

impl SigKernel for HostSigKernel {
    fn caps(&self) -> SigCaps {
        self.inner.lock().unwrap().caps
    }

    fn thread_pointer(&self) -> usize {
        self.pointer()
    }

    fn thread_handle(&self) -> u32 {
        FAKE_HANDLE
    }

    fn yield_now(&self) {}

    fn now_ns(&self) -> u64 {
        let mut inner = self.inner.lock().unwrap();
        inner.clock = inner.clock.saturating_add(10);
        inner.clock
    }

    fn sig_setup(&self, entry: usize, ctx_area_offset: usize, flags: u32) -> Result<(), Errno> {
        let _ = (ctx_area_offset, flags);
        raw::set_entry_function(entry);
        Ok(())
    }

    fn kick(&self, handle: u32, bits: u32) -> Result<(), Errno> {
        if self.failing("kick") {
            return Err(ENOSYS_ERR);
        }
        let _ = handle;
        self.kick_self(bits);
        Ok(())
    }

    fn sigreturn(&self) -> ! {
        std::panic::resume_unwind(Box::new(SigReturn))
    }

    fn resend(&self, sig: i32, info: &SiginfoT, mask: u64) -> Result<(), Errno> {
        self.record(Trace::Resend(sig));
        let tp = self.pointer();
        raw::mask_store(tp, mask);
        raw::pending_or(tp, crate::critical::signal_bit(sig));
        raw::store_pending_info(tp, info);
        self.kick_self(KICK_CHECK);
        Ok(())
    }

    fn proc_kill(&self, pid: i32, sig: i32, info: Option<&SiginfoT>) -> Result<(), Errno> {
        self.record(Trace::ProcKill(pid, sig));
        if let Some(record) = info {
            self.inner
                .lock()
                .unwrap()
                .proc_infos
                .push((pid, sig, *record));
        }
        if pid == FAKE_PID || pid == -FAKE_PID {
            let mut record = info.copied().unwrap_or_else(|| {
                crate::frame::user_siginfo(sig, raven_libc_abi::generated::signal::SI_USER)
            });
            record.si_signo = sig;
            crate::frame::set_si_pid(&mut record, FAKE_PID);
            crate::frame::set_si_uid(&mut record, FAKE_UID);
            self.push_pending(sig, record);
            self.kick_self(KICK_CHECK);
        }
        Ok(())
    }

    fn take_process_pending(&self, deliverable: u64) -> Option<(i32, SiginfoT)> {
        let mut inner = self.inner.lock().unwrap();
        let index = inner
            .pending
            .iter()
            .position(|(sig, _)| deliverable & crate::critical::signal_bit(*sig) != 0)?;
        Some(inner.pending.remove(index))
    }

    fn wait_for_kick(&self, deadline_ns: Option<u64>) -> Result<(), Errno> {
        self.record(Trace::Wait(deadline_ns));
        let mut inner = self.inner.lock().unwrap();
        if inner.thread.kick != 0 {
            inner.thread.kick = 0;
            return Ok(());
        }
        if deadline_ns.is_some() {
            return Err(ETIMEDOUT_ERR);
        }
        // No timer thread in the fake: a no-deadline wait would park forever. Return a spurious
        // wake and record it; tests that need a real kick arrange it before the call.
        inner.waited = true;
        Ok(())
    }

    fn terminate(&self, sig: i32, core: bool) -> ! {
        std::panic::resume_unwind(Box::new(Terminated { sig, core }))
    }
}

/// Whether the fake recorded a blocked wait.
#[must_use]
pub fn waited(kernel: &HostSigKernel) -> bool {
    kernel.inner.lock().unwrap().waited
}
