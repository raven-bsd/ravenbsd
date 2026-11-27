// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Thread objects and kernel-stack allocation.
//!
//! A thread owns a 32 KiB kernel stack with an unmapped guard page below it. Exiting threads
//! return their stacks to a cache bounded by the live-thread admission bound; reuse zeroes the
//! mapped frames and leaves the guard unmapped. The heap block is never returned because its
//! guard page mapping cannot be restored from the general heap.

use core::alloc::Layout;
use core::sync::atomic::{
    AtomicBool, AtomicI32, AtomicI64, AtomicU8, AtomicU32, AtomicU64, AtomicUsize, Ordering,
};

use alloc::alloc::alloc_zeroed;
use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec::Vec;
use spin::Mutex;

use raven_abi::errno::Errno;
use raven_rmm::VirtAddr;
use raven_sched_core::MAX_THREADS;

#[cfg(target_arch = "aarch64")]
use crate::arch::aarch64::fpu::FpuArea;
#[cfg(target_arch = "x86_64")]
use crate::arch::x86_64::fpu::FpuArea;
use crate::memory::paging;
use crate::process::Process;

/// Lowest priority (idle).
pub const PRIORITY_MAX: u8 = 31;
/// No inherited priority boost is active.
pub const NO_BOOST: u8 = u8::MAX;
/// Default priority for user and ordinary kernel threads.
pub const PRIORITY_DEFAULT: u8 = 16;
/// Priority of the per-CPU idle thread.
pub const PRIORITY_IDLE: u8 = PRIORITY_MAX;

/// Number of pages in one kernel stack (32 KiB).
///
/// P2g measured the aarch64 `PROC_FORK`/`PROC_SIGNAL` chain at 24,816 bytes including the 288-byte
/// exception frame: the fork/COW frames are deeper than the five-page stack left for them. S217
/// then measured the aarch64 `exec` VMA-insertion chain (`Vmas::insert` -> heap grow -> raven_kmem
/// -> raven_vmem carve) filling all seven mapped pages, so the exception entry wrote into the guard
/// page. NetBSD aarch64 reserves one 32 KiB `USPACE` for the uarea and trap frame
/// (`sys/arch/aarch64/include/vmparam.h`); eight mapped pages restore one page of headroom on both
/// architectures, where the same latent depth exists even though the x86_64 test path is shallower.
const KERNEL_STACK_PAGES: u64 = 8;
/// Page size.
const PAGE: u64 = 4096;
/// Total reserved pages, including the guard.
const STACK_RESERVED_PAGES: u64 = KERNEL_STACK_PAGES + 1;

static NEXT_THREAD_ID: AtomicU64 = AtomicU64::new(1);
/// 30-bit userspace thread ids (PG-2); zero is never handed out.
static NEXT_TID: AtomicU32 = AtomicU32::new(1);

/// Live non-idle threads (K527).
///
/// This is the kernel side of the run-queue admission proof: a thread that is not idle can be
/// a member of at most one run-queue level, so bounding the live thread count by
/// [`MAX_THREADS`] and sizing every level to the same bound means the queue can never refuse a
/// runnable thread. Idle threads are never enqueued and are excluded.
static LIVE_THREADS: AtomicUsize = AtomicUsize::new(0);

/// Every thread object ever created, weakly (S36b leaked-thread diagnostic only).
static ALL_THREADS: Mutex<Vec<alloc::sync::Weak<Thread>>> = Mutex::new(Vec::new());
/// Prune dead weak entries once the registry reaches this length.
const ALL_THREADS_PRUNE: usize = 256;

fn remember_thread(thread: &Arc<Thread>) {
    let mut registry = ALL_THREADS.lock();
    // A Weak keeps the ArcInner allocation alive after the value is dropped. The diagnostic
    // registry must not retain the 960-byte Thread block of every exited thread: prune weak
    // entries whose strong count is zero, keeping live and genuinely leaked threads visible.
    if registry.len() >= ALL_THREADS_PRUNE {
        registry.retain(|weak| weak.strong_count() > 0);
    }
    registry.push(Arc::downgrade(thread));
}

/// Count `target` words on every live thread's kernel stack (`s36-holders`).
///
/// A `Process` reference held by a blocked syscall sits in a kernel-stack local, outside every
/// object table the census can walk. Scanning the recorded stack range proves that owner and
/// names its count; it only runs in the diagnostic image.
pub fn stack_holders(target: usize) -> [u64; 2] {
    let mut threads = 0u64;
    let mut hits = 0u64;
    let word = core::mem::size_of::<usize>();
    for weak in ALL_THREADS.lock().iter() {
        let Some(thread) = weak.upgrade() else {
            continue;
        };
        let base = thread.kernel_stack_base as usize;
        let top = thread.kernel_stack_top as usize;
        if base == 0 || top <= base {
            continue;
        }
        threads = threads.saturating_add(1);
        let mut address = base;
        while address.saturating_add(word) <= top {
            // SAFETY: `base..top` is the thread's live kernel-stack allocation for as long as
            // the thread object is registered, and the scan stays inside it.
            if unsafe { *(address as *const usize) } == target {
                hits = hits.saturating_add(1);
            }
            address = address.saturating_add(word);
        }
    }
    [threads, hits]
}

/// Leaked thread objects (thread whose process is torn down) and their strong-ref histogram.
///
/// Returns `(leaked, external_refs_1, external_refs_2plus, blocked, ready, dead, token_set,
/// self_process)`. The caller's own exiting thread is counted in `self_process`, not in
/// `leaked`: it is still on its exit stack when the S36b row prints, and counting it made the
/// S170 churn gate fail on the row it was printing.
pub fn leaked_thread_objects() -> [u64; 8] {
    let mut stats = [0u64; 8];
    let self_process = crate::process::current();
    for weak in ALL_THREADS.lock().iter() {
        let Some(thread) = weak.upgrade() else {
            continue;
        };
        if !thread.process.torn_down.load(Ordering::Acquire) {
            continue;
        }
        if self_process
            .as_ref()
            .is_some_and(|own| Arc::ptr_eq(own, &thread.process))
        {
            stats[7] = stats[7].saturating_add(1);
            continue;
        }
        stats[0] = stats[0].saturating_add(1);
        match Arc::strong_count(&thread).saturating_sub(1) {
            0 | 1 => stats[1] = stats[1].saturating_add(1),
            _ => stats[2] = stats[2].saturating_add(1),
        }
        match thread.state() {
            ThreadState::Blocked => stats[3] = stats[3].saturating_add(1),
            ThreadState::Ready => stats[4] = stats[4].saturating_add(1),
            ThreadState::Dead => stats[5] = stats[5].saturating_add(1),
            ThreadState::Running => {}
        }
        if thread.block_token.load(Ordering::Acquire) != 0 {
            stats[6] = stats[6].saturating_add(1);
        }
    }
    stats
}

/// Holder census for leaked thread objects (S36 diagnostic).
///
/// Returns `(total, current, runqueue, sleepers, reap, threads_live, process_waiters, futex,
/// handles, multi_holder, unexplained, request_waiters)`. A weak `threads_live` entry is counted
/// separately: it is not a strong owner, but it shows the unregister path missed the thread.
/// `request_waiters` is the client wait queue of every queued or active `PendingRequest`; that
/// queue holds an `Arc<Thread>` until the request completes or is withdrawn.
pub fn thread_holder_stats() -> [u64; 12] {
    let Some(registry) = ALL_THREADS.try_lock() else {
        return [0u64; 12];
    };
    let self_process = crate::process::current();
    let threads: Vec<Arc<Thread>> = registry
        .iter()
        .filter_map(|weak| weak.upgrade())
        .filter(|thread| thread.process.torn_down.load(Ordering::Acquire))
        .filter(|thread| {
            !self_process
                .as_ref()
                .is_some_and(|own| Arc::ptr_eq(own, &thread.process))
        })
        .collect();
    drop(registry);
    let Some(processes) = crate::process::processes_try() else {
        return [0u64; 12];
    };
    let mut requests = Vec::new();
    for process in &processes {
        if let Some(queue) = process.scheme_queue.try_lock() {
            requests.extend(queue.iter().cloned());
        }
        if let Some(queue) = process.active_requests.try_lock() {
            requests.extend(queue.iter().cloned());
        }
        if let Some(queue) = process.client_requests.try_lock() {
            requests.extend(queue.iter().cloned());
        }
    }
    let mut stats = [0u64; 12];
    for thread in &threads {
        stats[0] = stats[0].saturating_add(1);
        let mut holders = 0u64;
        let sched = crate::sched::holder_state(thread.id);
        for (index, count) in sched.iter().enumerate() {
            if *count != 0 {
                if let Some(entry) = stats.get_mut(index.saturating_add(1)) {
                    *entry = entry.saturating_add(1);
                }
                holders = holders.saturating_add(1);
            }
        }
        if let Some(live) = thread.process.threads_live.try_lock()
            && live.iter().any(|weak| {
                weak.upgrade()
                    .is_some_and(|entry| Arc::ptr_eq(&entry, thread))
            })
        {
            stats[5] = stats[5].saturating_add(1);
            holders = holders.saturating_add(1);
        }
        // Handles before queues: the process teardown path holds a process's handle table and then
        // takes wait-queue locks, so the census must not invert that order.
        let handle = processes.iter().any(|process| {
            let Some(handles) = process.handles.try_lock() else {
                return false;
            };
            let mut found = false;
            handles.for_each_live(|_, object, _, _| {
                if let crate::process::handle::Object::Thread(target) = object
                    && Arc::ptr_eq(target, thread)
                {
                    found = true;
                }
            });
            found
        });
        if handle {
            stats[8] = stats[8].saturating_add(1);
            holders = holders.saturating_add(1);
        }
        let process_waiters = processes.iter().any(|process| {
            [
                &process.waiters,
                &process.signal_waiters,
                &process.child_waiters,
                &process.scheme_waiters,
            ]
            .iter()
            .any(|queue| queue.try_contains_thread(thread.id) == Some(true))
        });
        if process_waiters {
            stats[6] = stats[6].saturating_add(1);
            holders = holders.saturating_add(1);
        }
        if crate::syscall::futex::try_contains_thread(thread.id) == Some(true) {
            stats[7] = stats[7].saturating_add(1);
            holders = holders.saturating_add(1);
        }
        if requests
            .iter()
            .any(|request| request.waiters.try_contains_thread(thread.id) == Some(true))
        {
            stats[11] = stats[11].saturating_add(1);
            holders = holders.saturating_add(1);
        }
        if holders > 1 {
            stats[9] = stats[9].saturating_add(1);
        }
        if holders == 0 {
            stats[10] = stats[10].saturating_add(1);
        }
    }
    stats
}

/// Admission denials since boot (K530 churn diagnostic).
static SLOT_DENIES: AtomicUsize = AtomicUsize::new(0);
/// Slots released since boot (K530 churn diagnostic).
static SLOTS_DROPPED: AtomicUsize = AtomicUsize::new(0);

/// RAII reservation for the kernel-wide thread bound.
struct ThreadSlot {
    counted: bool,
}

impl ThreadSlot {
    /// Reserve one non-idle thread slot, or fail the thread creation with `EAGAIN`.
    fn reserve() -> Result<Self, Errno> {
        let live = LIVE_THREADS.fetch_add(1, Ordering::AcqRel);
        if live >= MAX_THREADS {
            LIVE_THREADS.fetch_sub(1, Ordering::AcqRel);
            let denied = SLOT_DENIES.fetch_add(1, Ordering::AcqRel).saturating_add(1);
            if denied == 1 || denied.is_multiple_of(1_000) {
                crate::kprintln!(
                    "slot: deny n={denied} live={live} max={MAX_THREADS} dropped={}",
                    SLOTS_DROPPED.load(Ordering::Acquire)
                );
            }
            return Err(Errno::EAGAIN);
        }
        Ok(Self { counted: true })
    }

    /// The per-CPU idle threads are not run-queue members and do not consume the bound.
    const fn idle() -> Self {
        Self { counted: false }
    }
}

impl Drop for ThreadSlot {
    fn drop(&mut self) {
        if self.counted {
            LIVE_THREADS.fetch_sub(1, Ordering::AcqRel);
            SLOTS_DROPPED.fetch_add(1, Ordering::AcqRel);
        }
    }
}

/// Number of live non-idle threads (diagnostic and the K527 bound).
#[allow(dead_code)] // used by the scheduler saturation self-test and diagnostics
pub fn live_threads() -> usize {
    LIVE_THREADS.load(Ordering::Acquire)
}

/// Thread-slot drops since boot (diagnostic).
pub fn slots_dropped() -> usize {
    SLOTS_DROPPED.load(Ordering::Acquire)
}

/// Every Thread object still reachable from the weak registry.
///
/// Returns `(alive, strong_refs_1, strong_refs_2plus, dead_state)`.
pub fn all_thread_census() -> [u64; 4] {
    let mut stats = [0u64; 4];
    for weak in ALL_THREADS.lock().iter() {
        let Some(thread) = weak.upgrade() else {
            continue;
        };
        stats[0] = stats[0].saturating_add(1);
        if Arc::strong_count(&thread).saturating_sub(1) <= 1 {
            stats[1] = stats[1].saturating_add(1);
        } else {
            stats[2] = stats[2].saturating_add(1);
        }
        if thread.state() == ThreadState::Dead {
            stats[3] = stats[3].saturating_add(1);
        }
    }
    stats
}

/// Mask keeping a tid in the musl lock-word / PG-2 30-bit range.
const TID_MASK: u32 = 0x3fff_ffff;

fn next_tid() -> u32 {
    loop {
        let tid = NEXT_TID.fetch_add(1, Ordering::Relaxed) & TID_MASK;
        if tid != 0 {
            return tid;
        }
    }
}

/// Thread run state.
#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ThreadState {
    /// Currently executing on the CPU.
    Running = 0,
    /// On a run queue, waiting for the CPU.
    Ready = 1,
    /// Waiting on a sleep list or wait queue.
    Blocked = 2,
    /// Exited; never scheduled again.
    Dead = 3,
}

impl ThreadState {
    fn from_raw(value: u8) -> ThreadState {
        match value {
            0 => ThreadState::Running,
            1 => ThreadState::Ready,
            2 => ThreadState::Blocked,
            _ => ThreadState::Dead,
        }
    }
}

/// Scheduler parameters reserved for the real-time class (DR-20).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SchedContext {
    /// Static priority, 0 (highest) ..= 31 (lowest).
    pub priority: u8,
    /// Reserved budget in nanoseconds; zero means no RT budget.
    pub budget_ns: u64,
    /// Reserved period in nanoseconds; zero means no RT period.
    pub period_ns: u64,
}

impl SchedContext {
    /// A plain fair-class context at `priority`.
    pub const fn fair(priority: u8) -> Self {
        Self {
            priority,
            budget_ns: 0,
            period_ns: 0,
        }
    }
}

/// A schedulable kernel or user thread.
///
/// The user-thread fields (`user_entry`, `user_stack`, `arg`, `cr3`, and `id` for process
/// accounting) are only read by the x86-64 user path; aarch64 has kernel threads until S14c.
#[cfg_attr(target_arch = "aarch64", allow(dead_code))]
pub struct Thread {
    /// Monotonic thread id.
    pub id: u64,
    /// Stable 30-bit nonzero userspace tid (PG-2).
    pub tid: u32,
    /// User word cleared on thread exit (zero disables it).
    pub clear_tid: AtomicU64,
    /// Diagnostic name.
    #[allow(dead_code)] // used by diagnostics in later tickets
    pub name: String,
    /// Owning process (the kernel process for kernel threads).
    pub process: Arc<Process>,
    state: AtomicU8,
    /// Static priority.
    pub priority: AtomicU8,
    /// Inherited turnstile/donation boost, [`NO_BOOST`] when no donor is active.
    pub boost: AtomicU8,
    /// Reserved scheduling context (DR-20).
    #[allow(dead_code)] // placeholder read by the future RT class
    pub sched: SchedContext,
    /// Base of the reserved kernel-stack allocation (guard page).
    #[allow(dead_code)] // ownership marker for the future stack cache
    pub kernel_stack_base: u64,
    /// Top of the mapped kernel stack.
    pub kernel_stack_top: u64,
    saved_rsp: core::cell::UnsafeCell<u64>,
    /// Architectural FP/SIMD save area.
    pub fpu: core::cell::UnsafeCell<FpuArea>,
    /// True while the user FP state is parked in `fpu` (thread is inside a syscall).
    #[cfg(target_arch = "x86_64")]
    pub fpu_saved: AtomicBool,
    /// True for a ring-3 thread.
    pub user: bool,
    /// User entry point (zero for kernel threads).
    pub user_entry: u64,
    /// Initial user stack pointer (zero for kernel threads).
    pub user_stack: u64,
    /// First-argument register value.
    pub arg: u64,
    /// Kernel entry function (kernel threads).
    pub kernel_entry: Option<fn()>,
    /// Address-space root used while this thread runs.
    pub cr3: u64,
    /// Exit code once the thread is dead.
    pub exit_code: AtomicI32,
    /// Saved preemption depth, transferred by the context switch.
    pub preempt_count: AtomicU32,
    /// Remaining timer ticks in the current slice.
    pub slice_ticks: AtomicU32,
    /// Token identifying the current blocking operation.
    pub block_token: AtomicU64,
    /// Serialises the state/`pending_wake` handshake between the waiter and a waker.
    ///
    /// The pair is not atomic on its own: a waker that reads `Running` may store
    /// `pending_wake` after the waiter has passed its final check. Holding this lock across
    /// both sides' read-modify sequences closes that window (K391).
    pub block_lock: AtomicBool,
    /// Status delivered by the wakeup (0 or `-errno`).
    pub wake_status: AtomicI64,
    /// A wakeup arrived before the thread reached `block_current`.
    pub pending_wake: AtomicBool,
    /// True for the per-CPU idle thread.
    pub idle: bool,
    /// CPU this thread last ran on (used to route wakeups).
    pub last_cpu: AtomicU32,
    /// Pinned threads are never stolen by another CPU (deterministic SMP self-test workers).
    pub pinned: AtomicBool,
    /// Owning `Arc<Thread>` parked while this thread is switched out.
    ///
    /// `schedule_locked` moves its self-reference here before the context switch and takes it
    /// back on resume; the deferred reaper drops it when the terminal-process filter removes the
    /// thread from the run queue without resuming it. Zero while the thread is running.
    pub parked: AtomicUsize,
    /// User TLS base (`IA32_FS_BASE` / `TPIDR_EL0`), installed on switch-in.
    pub tls_base: AtomicU64,
    /// Monotonic timestamp when this user thread last entered user mode; zero in kernel mode.
    pub user_enter_ns: AtomicU64,
    /// Sticky thread-kick bits (PG-8).
    pub kick_bits: AtomicU32,
    /// Signals whose disposition is caught and that are pending for this thread only.
    ///
    /// FreeBSD keeps a thread-directed queue (`td_sigqueue`) beside the process queue
    /// (`p_sigqueue`); `sigtd`/`issignal` merge both, and `postsig` consumes the thread queue
    /// first. A thread-directed send (`THREAD_SIGQUEUE`) lands here.
    pub pending_signals: Mutex<alloc::collections::VecDeque<crate::process::PendingSignal>>,
    /// Sticky reason for an interruptible wait: a caught signal is queued for this thread.
    ///
    /// `sched::wake` must not downgrade this `EINTR` to a bare successful wake just because the
    /// retired `SIG_SETUP` entry is absent (C19 kernel `sigaction` model).
    pub signal_wake: AtomicBool,
    /// True while the thread is in an interruptible blocking wait.
    pub interruptible: AtomicBool,
    /// Registered `SIG_SETUP` entry point; zero means no signal delivery (S44).
    pub signal_entry: AtomicU64,
    /// Registered context-area offset relative to the TLS base.
    pub signal_ctx_offset: AtomicU64,
    /// Absolute registered context area (non-zero overrides `signal_ctx_offset`; K443).
    pub signal_ctx_abs: AtomicU64,
    /// Registered `SIG_SETUP` flags.
    pub signal_flags: AtomicU32,
    /// Resolved user context area of the signal currently being handled.
    pub signal_active_ctx: AtomicU64,
    /// Signal-handler nesting depth; zero outside a signal.
    pub signal_depth: AtomicU32,
    /// User signal mask restored by the last `SIGRESEND` (passed to the entry).
    pub signal_mask: AtomicU64,
    /// Per-thread blocked mask for the kernel sigaction model (signal 1 is bit 0).
    pub sig_blocked: AtomicU64,
    /// Alternate signal stack base (`ss_sp`); zero while disabled.
    pub sig_alt_sp: AtomicU64,
    /// Alternate signal stack size (`ss_size`).
    pub sig_alt_size: AtomicU64,
    /// Alternate signal stack flags (`SS_DISABLE`/`SS_ONSTACK`).
    pub sig_alt_flags: AtomicU32,
    /// True while execution is on the alternate signal stack.
    ///
    /// One flag, not a per-frame stack: the NetBSD `l_sigstk.ss_flags` model keeps
    /// `SS_ONSTACK` set across a nested delivery that runs on the normal stack, and each
    /// `ucontext_t` records the previous state in `uc_flags` (`_UC_SETSTACK`/`_UC_CLRSTACK`)
    /// so `SIGRETURN` restores it. There is no kernel frame pointer or "in handler" boolean:
    /// the return path derives and validates the frame from the trap frame itself, so nested
    /// frames restore independently.
    pub sig_on_altstack: AtomicBool,
    /// A `SIGRESEND` re-entry is waiting for the return-to-user path.
    pub resend: AtomicBool,
    /// Current cross-process trace id (`R-OBS-6`); zero means "not traced".
    pub trace_id: AtomicU64,
    /// Kernel-wide thread-bound reservation (K527); released when the thread object drops.
    _thread_slot: ThreadSlot,
}

// SAFETY: all interior mutability is accessed from scheduler critical sections (interrupts off)
// or through atomics. `saved_rsp` is only read/written by the context-switch path on one CPU.
unsafe impl Send for Thread {}
// SAFETY: same argument as `Send`; the scheduler serialises shared access.
unsafe impl Sync for Thread {}

impl Thread {
    /// Current state.
    #[inline]
    pub fn state(&self) -> ThreadState {
        ThreadState::from_raw(self.state.load(Ordering::SeqCst))
    }

    /// Run-queue priority for this thread.
    ///
    /// User threads derive it from the owning process's POSIX nice value, so `setpriority`
    /// reaches every later enqueue/wake. Kernel and idle threads keep their static priority.
    pub fn effective_priority(&self) -> u8 {
        let base = if self.user && !self.idle {
            crate::process::priority::nice_to_priority(self.process.nice())
        } else {
            self.priority.load(Ordering::Acquire)
        };
        let boost = self.boost.load(Ordering::Acquire);
        if boost == NO_BOOST {
            base
        } else {
            base.min(boost)
        }
    }

    /// Apply an inherited priority boost (0 is the most urgent).
    #[allow(dead_code)] // L3 is the first consumer
    #[inline]
    pub fn set_boost(&self, priority: u8) {
        self.boost.store(priority, Ordering::Release);
    }

    /// Clear the inherited boost and return to the base priority.
    #[allow(dead_code)] // L3 is the first consumer
    #[inline]
    pub fn clear_boost(&self) {
        self.boost.store(NO_BOOST, Ordering::Release);
    }

    /// Change state.
    #[inline]
    pub fn set_state(&self, state: ThreadState) {
        self.state.store(state as u8, Ordering::SeqCst);
    }

    /// Saved kernel stack pointer used by `sched_switch`.
    #[inline]
    pub fn saved_rsp(&self) -> u64 {
        // SAFETY: the value is a plain stack pointer published by the scheduler.
        unsafe { self.saved_rsp.get().read() }
    }

    /// The current user TLS base.
    #[inline]
    pub fn tls_base(&self) -> u64 {
        self.tls_base.load(Ordering::Acquire)
    }

    /// Store the user TLS base; the next switch to this thread installs it.
    #[inline]
    pub fn set_tls_base(&self, value: u64) {
        self.tls_base.store(value, Ordering::Release);
    }

    /// Store the saved kernel stack pointer.
    ///
    /// # Safety
    ///
    /// Only the scheduler and the initial thread setup may publish a stack pointer.
    #[inline]
    pub unsafe fn set_saved_rsp(&self, value: u64) {
        // SAFETY: the caller owns the context-switch handoff.
        unsafe {
            self.saved_rsp.get().write(value);
        }
    }

    /// Pointer to the saved stack slot for the assembly switch.
    #[inline]
    pub fn saved_rsp_slot(&self) -> *mut u64 {
        self.saved_rsp.get()
    }

    /// Raw pointer to the thread's FP/SIMD save area.
    ///
    /// `FpuArea` is `repr(C)` with the byte array as its first field, so the area address is the
    /// struct address. Callers must own the thread's FP/SIMD state (scheduler critical section).
    #[inline]
    pub fn fpu_area_ptr(&self) -> *mut u8 {
        self.fpu.get() as *mut u8
    }

    /// Raw const pointer to the thread's FP/SIMD save area.
    #[inline]
    pub fn fpu_area_const_ptr(&self) -> *const u8 {
        self.fpu.get() as *const u8
    }

    fn new(
        name: String,
        process: Arc<Process>,
        priority: u8,
        user: bool,
        cr3: u64,
        idle: bool,
    ) -> Result<Arc<Thread>, Errno> {
        if !crate::sched::runqueue::valid_priority(priority) {
            return Err(Errno::EINVAL);
        }
        // K527: enforce the bound before allocating; the reservation drops on every error path
        // below, and later when the thread object itself drops.
        let slot = ThreadSlot::reserve()?;
        let (base, top) = allocate_kernel_stack().ok_or(Errno::ENOMEM)?;
        let thread = Arc::new(Thread {
            id: NEXT_THREAD_ID.fetch_add(1, Ordering::Relaxed),
            tid: next_tid(),
            clear_tid: AtomicU64::new(0),
            name,
            process,
            state: AtomicU8::new(ThreadState::Ready as u8),
            priority: AtomicU8::new(priority),
            boost: AtomicU8::new(NO_BOOST),
            sched: SchedContext::fair(priority),
            kernel_stack_base: base,
            kernel_stack_top: top,
            saved_rsp: core::cell::UnsafeCell::new(0),
            fpu: core::cell::UnsafeCell::new(FpuArea::new()),
            #[cfg(target_arch = "x86_64")]
            fpu_saved: AtomicBool::new(false),
            user,
            user_entry: 0,
            user_stack: 0,
            arg: 0,
            kernel_entry: None,
            cr3,
            exit_code: AtomicI32::new(0),
            preempt_count: AtomicU32::new(0),
            slice_ticks: AtomicU32::new(crate::sched::SLICE_TICKS),
            block_token: AtomicU64::new(0),
            block_lock: AtomicBool::new(false),
            wake_status: AtomicI64::new(0),
            pending_wake: AtomicBool::new(false),
            idle,
            last_cpu: AtomicU32::new(0),
            pinned: AtomicBool::new(false),
            parked: AtomicUsize::new(0),
            tls_base: AtomicU64::new(0),
            user_enter_ns: AtomicU64::new(0),
            kick_bits: AtomicU32::new(0),
            pending_signals: Mutex::new(alloc::collections::VecDeque::new()),
            signal_wake: AtomicBool::new(false),
            interruptible: AtomicBool::new(false),
            signal_entry: AtomicU64::new(0),
            signal_ctx_offset: AtomicU64::new(0),
            signal_ctx_abs: AtomicU64::new(0),
            signal_flags: AtomicU32::new(0),
            signal_active_ctx: AtomicU64::new(0),
            signal_depth: AtomicU32::new(0),
            signal_mask: AtomicU64::new(0),
            sig_blocked: AtomicU64::new(0),
            sig_alt_sp: AtomicU64::new(0),
            sig_alt_size: AtomicU64::new(0),
            sig_alt_flags: AtomicU32::new(crate::process::SS_DISABLE),
            sig_on_altstack: AtomicBool::new(false),
            resend: AtomicBool::new(false),
            trace_id: AtomicU64::new(0),
            _thread_slot: slot,
        });
        Ok(thread)
    }

    /// Create a ring-3 thread in `process`.
    pub fn new_user(
        name: String,
        process: Arc<Process>,
        entry: u64,
        stack_top: u64,
        arg: u64,
        cr3: u64,
        priority: u8,
    ) -> Result<Arc<Thread>, Errno> {
        let thread = Self::new(name, Arc::clone(&process), priority, true, cr3, false)?;
        // SAFETY: the thread is not shared yet, so mutable access through the raw pointer is
        // exclusive. All fields are plain data published before the first schedule.
        unsafe {
            let pointer = Arc::as_ptr(&thread) as *mut Thread;
            (*pointer).user_entry = entry;
            (*pointer).user_stack = stack_top;
            (*pointer).arg = arg;
        }
        let saved = crate::sched::context::prepare_stack(thread.kernel_stack_top)?;
        // SAFETY: the thread has not been enqueued or observed by another context.
        unsafe {
            thread.set_saved_rsp(saved);
        }
        remember_thread(&thread);
        Ok(thread)
    }

    /// Copy the creating thread's blocked mask before the thread is published.
    ///
    /// POSIX `pthread_create` and NetBSD `pthread_create.3` ("The signal mask is inherited from
    /// the creating thread"); FreeBSD `kern_thread.c` `thread_alloc` copies the target process
    /// mask. The new thread is not on any run queue yet, so the write is private; `sched::enqueue`
    /// publishes it under the run-queue lock.
    pub fn inherit_signal_mask(&self, creator: &Thread) {
        self.sig_blocked.store(
            creator.sig_blocked.load(Ordering::Acquire),
            Ordering::Release,
        );
    }

    /// Create a kernel thread that starts at `entry`.
    pub fn new_kernel(
        name: String,
        process: Arc<Process>,
        entry: fn(),
        cr3: u64,
        priority: u8,
    ) -> Result<Arc<Thread>, Errno> {
        let thread = Self::new(name, Arc::clone(&process), priority, false, cr3, false)?;
        // SAFETY: the thread is not shared yet; see `new_user`.
        unsafe {
            let pointer = Arc::as_ptr(&thread) as *mut Thread;
            (*pointer).kernel_entry = Some(entry);
        }
        let saved = crate::sched::context::prepare_stack(thread.kernel_stack_top)?;
        // SAFETY: the thread is not enqueued yet.
        unsafe {
            thread.set_saved_rsp(saved);
        }
        remember_thread(&thread);
        Ok(thread)
    }

    /// Create the in-place idle thread for the boot CPU.
    pub fn new_idle(process: Arc<Process>, current_rsp: u64, cr3: u64) -> Arc<Thread> {
        let thread = Arc::new(Thread {
            id: NEXT_THREAD_ID.fetch_add(1, Ordering::Relaxed),
            tid: next_tid(),
            clear_tid: AtomicU64::new(0),
            name: String::from("idle"),
            process,
            state: AtomicU8::new(ThreadState::Running as u8),
            priority: AtomicU8::new(PRIORITY_IDLE),
            boost: AtomicU8::new(NO_BOOST),
            sched: SchedContext::fair(PRIORITY_IDLE),
            kernel_stack_base: 0,
            kernel_stack_top: current_rsp & !0xf,
            saved_rsp: core::cell::UnsafeCell::new(0),
            fpu: core::cell::UnsafeCell::new(FpuArea::new()),
            #[cfg(target_arch = "x86_64")]
            fpu_saved: AtomicBool::new(false),
            user: false,
            user_entry: 0,
            user_stack: 0,
            arg: 0,
            kernel_entry: None,
            cr3,
            exit_code: AtomicI32::new(0),
            preempt_count: AtomicU32::new(0),
            slice_ticks: AtomicU32::new(crate::sched::SLICE_TICKS),
            block_token: AtomicU64::new(0),
            block_lock: AtomicBool::new(false),
            wake_status: AtomicI64::new(0),
            pending_wake: AtomicBool::new(false),
            idle: true,
            last_cpu: AtomicU32::new(0),
            pinned: AtomicBool::new(false),
            parked: AtomicUsize::new(0),
            tls_base: AtomicU64::new(0),
            user_enter_ns: AtomicU64::new(0),
            kick_bits: AtomicU32::new(0),
            pending_signals: Mutex::new(alloc::collections::VecDeque::new()),
            signal_wake: AtomicBool::new(false),
            interruptible: AtomicBool::new(false),
            signal_entry: AtomicU64::new(0),
            signal_ctx_offset: AtomicU64::new(0),
            signal_ctx_abs: AtomicU64::new(0),
            signal_flags: AtomicU32::new(0),
            signal_active_ctx: AtomicU64::new(0),
            signal_depth: AtomicU32::new(0),
            signal_mask: AtomicU64::new(0),
            sig_blocked: AtomicU64::new(0),
            sig_alt_sp: AtomicU64::new(0),
            sig_alt_size: AtomicU64::new(0),
            sig_alt_flags: AtomicU32::new(crate::process::SS_DISABLE),
            sig_on_altstack: AtomicBool::new(false),
            resend: AtomicBool::new(false),
            trace_id: AtomicU64::new(0),
            _thread_slot: ThreadSlot::idle(),
        });
        remember_thread(&thread);
        thread
    }

    /// Create the idle thread for an application processor.
    ///
    /// The AP is already executing on the returned kernel stack when this object is published, so
    /// the stack is not prepared with [`crate::sched::context::prepare_stack`]: the first
    /// `sched_switch` saves the live stack pointer.
    pub fn new_idle_ap(process: Arc<Process>, cr3: u64) -> Result<Arc<Thread>, Errno> {
        let (base, top) = allocate_kernel_stack().ok_or(Errno::ENOMEM)?;
        let thread = Arc::new(Thread {
            id: NEXT_THREAD_ID.fetch_add(1, Ordering::Relaxed),
            tid: next_tid(),
            clear_tid: AtomicU64::new(0),
            name: String::from("idle-ap"),
            process,
            state: AtomicU8::new(ThreadState::Running as u8),
            priority: AtomicU8::new(PRIORITY_IDLE),
            boost: AtomicU8::new(NO_BOOST),
            sched: SchedContext::fair(PRIORITY_IDLE),
            kernel_stack_base: base,
            kernel_stack_top: top,
            saved_rsp: core::cell::UnsafeCell::new(0),
            fpu: core::cell::UnsafeCell::new(FpuArea::new()),
            #[cfg(target_arch = "x86_64")]
            fpu_saved: AtomicBool::new(false),
            user: false,
            user_entry: 0,
            user_stack: 0,
            arg: 0,
            kernel_entry: None,
            cr3,
            exit_code: AtomicI32::new(0),
            preempt_count: AtomicU32::new(0),
            slice_ticks: AtomicU32::new(crate::sched::SLICE_TICKS),
            block_token: AtomicU64::new(0),
            block_lock: AtomicBool::new(false),
            wake_status: AtomicI64::new(0),
            pending_wake: AtomicBool::new(false),
            idle: true,
            last_cpu: AtomicU32::new(0),
            pinned: AtomicBool::new(false),
            parked: AtomicUsize::new(0),
            tls_base: AtomicU64::new(0),
            user_enter_ns: AtomicU64::new(0),
            kick_bits: AtomicU32::new(0),
            pending_signals: Mutex::new(alloc::collections::VecDeque::new()),
            signal_wake: AtomicBool::new(false),
            interruptible: AtomicBool::new(false),
            signal_entry: AtomicU64::new(0),
            signal_ctx_offset: AtomicU64::new(0),
            signal_ctx_abs: AtomicU64::new(0),
            signal_flags: AtomicU32::new(0),
            signal_active_ctx: AtomicU64::new(0),
            signal_depth: AtomicU32::new(0),
            signal_mask: AtomicU64::new(0),
            sig_blocked: AtomicU64::new(0),
            sig_alt_sp: AtomicU64::new(0),
            sig_alt_size: AtomicU64::new(0),
            sig_alt_flags: AtomicU32::new(crate::process::SS_DISABLE),
            sig_on_altstack: AtomicBool::new(false),
            resend: AtomicBool::new(false),
            trace_id: AtomicU64::new(0),
            _thread_slot: ThreadSlot::idle(),
        });
        remember_thread(&thread);
        Ok(thread)
    }
}

/// Freed stacks awaiting reuse (S36).  A cached stack keeps its guard page unmapped; reuse
/// skips the unmap.  The cache is deliberately bounded by the live-thread admission bound
/// ([`MAX_THREADS`]) and never hands memory back to the heap: the heap block's guard page mapping
/// was removed and cannot be restored without its lost physical frame.  Every stack freed by a
/// thread that was admitted under the bound therefore has a cache slot, so no `(base, top)` is
/// dropped (K530).
static STACK_CACHE: Mutex<Vec<(u64, u64)>> = Mutex::new(Vec::new());
/// Maximum cached stacks. The sched-core admission bound is the only way to exceed it; every
/// live thread has one stack and every freed stack is cached here.
const STACK_CACHE_MAX: usize = MAX_THREADS;
/// Stacks freshly carved from the heap since boot (diagnostic counter).
static STACK_FRESH: AtomicU64 = AtomicU64::new(0);
/// Freed stack pairs the cache refused (the K530 defect); should stay zero.
static STACK_DROPS: AtomicU64 = AtomicU64::new(0);
/// One-shot `s69-stackcheck` reuse probe.
static STACK_SELFTEST_DONE: AtomicBool = AtomicBool::new(false);

/// One-shot `s69-stackcheck` reuse probe: dirty a freed stack, reallocate it, and require the
/// reused frames to be zero. Gated by the fixture command line so normal boots are unchanged.
fn stack_selftest_once() {
    if STACK_SELFTEST_DONE.swap(true, Ordering::AcqRel) {
        return;
    }
    if !paging::cmdline()
        .split_ascii_whitespace()
        .any(|token| token == "s69-stackcheck")
    {
        return;
    }
    let Some((base, top)) = allocate_kernel_stack() else {
        crate::kprintln!("stack-selftest: alloc fail");
        return;
    };
    let guard_first = guard_unmapped(base);
    let Some(start) = base.checked_add(PAGE) else {
        crate::kprintln!("stack-selftest: address fail");
        return;
    };
    let Some(length) = usize::try_from(top.saturating_sub(start)).ok() else {
        crate::kprintln!("stack-selftest: length fail");
        return;
    };
    // SAFETY: the mapped stack region is `start..top`; the guard below `start` is not touched.
    unsafe { core::ptr::write_bytes(start as *mut u8, 0xAA, length) };
    free_kernel_stack(base, top);
    let Some((base2, top2)) = allocate_kernel_stack() else {
        crate::kprintln!("stack-selftest: realloc fail");
        return;
    };
    let start2 = match base2.checked_add(PAGE) {
        Some(value) => value,
        None => return,
    };
    // The reuse path must keep the guard page unmapped; a remapped guard would let an overflow
    // run into the neighbouring heap block instead of faulting.
    let guard_reused = guard_unmapped(base2);
    // SAFETY: the reused stack's mapped region is `start2..top2` and is owned by this probe.
    let zero = unsafe { core::slice::from_raw_parts(start2 as *const u8, length) }
        .iter()
        .all(|byte| *byte == 0);
    free_kernel_stack(base2, top2);
    if guard_first && guard_reused {
        crate::kprintln!("stack-selftest: guard-unmapped ok");
    } else {
        crate::kprintln!("stack-selftest: guard-mapped");
    }
    if zero {
        crate::kprintln!("stack-selftest: reuse-zero ok");
    } else {
        crate::kprintln!("stack-selftest: reuse-stale");
    }
}

/// True when the guard page at `base` carries no mapping, so a stack overflow faults.
fn guard_unmapped(base: u64) -> bool {
    let Some(address) = VirtAddr::new(base) else {
        return false;
    };
    paging::with_kernel_mapper(|mapper| mapper.translate(address).is_none()).unwrap_or(false)
}

/// Allocate a page-aligned kernel stack and unmap its lowest page as a guard.
///
/// Returns `(base, top)`, or `None` when the heap or the kernel mapper fails.  A cached stack is
/// reused first; its guard is already unmapped.
pub fn allocate_kernel_stack() -> Option<(u64, u64)> {
    stack_selftest_once();
    if let Some((base, top)) = STACK_CACHE.lock().pop() {
        // A cached stack was owned by a thread that may have left data in its kernel frames.
        // Zero the mapped stack pages (never the unmapped guard at `base`) before the next
        // owner starts on it (K530 reuse-content row).
        let start = base.checked_add(PAGE)?;
        let length = usize::try_from(top.checked_sub(start)?).ok()?;
        if length != 0 {
            // SAFETY: `start..top` is the mapped stack region of this cached stack; the guard
            // page below `start` is deliberately excluded. The cache lock serialises ownership.
            unsafe { core::ptr::write_bytes(start as *mut u8, 0, length) };
        }
        return Some((base, top));
    }
    let pages = STACK_RESERVED_PAGES;
    let size = usize::try_from(pages.checked_mul(PAGE)?).ok()?;
    let layout = Layout::from_size_align(size, PAGE as usize).ok()?;
    // SAFETY: the layout is valid and the kernel heap is initialised before any thread exists.
    let pointer = unsafe { alloc_zeroed(layout) };
    if pointer.is_null() {
        return None;
    }
    let base = pointer as u64;
    let guard = VirtAddr::new(base)?;
    let unmapped = paging::with_kernel_mapper(|mapper| mapper.unmap(guard)).ok()?;
    unmapped.ok()?;
    let top = base.checked_add(STACK_RESERVED_PAGES.checked_mul(PAGE)?)?;
    STACK_FRESH.fetch_add(1, Ordering::Relaxed);
    Some((base, top))
}

/// Return a stack for reuse.  The caller must no longer be running on it.
pub fn free_kernel_stack(base: u64, top: u64) {
    if base == 0 || top <= base {
        return;
    }
    let mut cache = STACK_CACHE.lock();
    if cache.len() < STACK_CACHE_MAX {
        cache.push((base, top));
    } else {
        let dropped = STACK_DROPS
            .fetch_add(1, Ordering::Relaxed)
            .saturating_add(1);
        if dropped == 1 {
            crate::kprintln!("stack-cache: DROPPED base={base:#x} top={top:#x}");
        }
    }
}

/// Stacks freshly carved from the heap (diagnostic).
pub fn stack_fresh() -> u64 {
    STACK_FRESH.load(Ordering::Relaxed)
}

/// Stacks currently cached for reuse (diagnostic).
pub fn stack_cached() -> u64 {
    u64::try_from(STACK_CACHE.lock().len()).unwrap_or(0)
}
