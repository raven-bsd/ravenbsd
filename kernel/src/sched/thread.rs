// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Thread objects and kernel-stack allocation.
//!
//! A thread owns a 16 KiB kernel stack with an unmapped guard page below it. The stack is never
//! freed in S09: exiting threads leave their stacks mapped for the kernel's lifetime, which is
//! safe (no use-after-free) and bounded by the test workload. A later ticket adds a stack cache.

use core::alloc::Layout;
use core::sync::atomic::{
    AtomicBool, AtomicI32, AtomicI64, AtomicU8, AtomicU32, AtomicU64, Ordering,
};

use alloc::alloc::alloc_zeroed;
use alloc::string::String;
use alloc::sync::Arc;

use raven_abi::errno::Errno;
use raven_rmm::VirtAddr;

use crate::memory::paging;
use crate::process::Process;

/// Lowest priority (idle).
pub const PRIORITY_MAX: u8 = 31;
/// Default priority for user and ordinary kernel threads.
pub const PRIORITY_DEFAULT: u8 = 16;
/// Priority of the per-CPU idle thread.
pub const PRIORITY_IDLE: u8 = PRIORITY_MAX;

/// Number of pages in one kernel stack (16 KiB).
const KERNEL_STACK_PAGES: u64 = 4;
/// Page size.
const PAGE: u64 = 4096;
/// Total reserved pages, including the guard.
const STACK_RESERVED_PAGES: u64 = KERNEL_STACK_PAGES + 1;

static NEXT_THREAD_ID: AtomicU64 = AtomicU64::new(1);

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
pub struct Thread {
    /// Monotonic thread id.
    pub id: u64,
    /// Diagnostic name.
    #[allow(dead_code)] // used by diagnostics in later tickets
    pub name: String,
    /// Owning process (the kernel process for kernel threads).
    pub process: Arc<Process>,
    state: AtomicU8,
    /// Static priority.
    pub priority: u8,
    /// Reserved scheduling context (DR-20).
    #[allow(dead_code)] // placeholder read by the future RT class
    pub sched: SchedContext,
    /// Base of the reserved kernel-stack allocation (guard page).
    #[allow(dead_code)] // ownership marker for the future stack cache
    pub kernel_stack_base: u64,
    /// Top of the mapped kernel stack.
    pub kernel_stack_top: u64,
    saved_rsp: core::cell::UnsafeCell<u64>,
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
        let (base, top) = allocate_kernel_stack().ok_or(Errno::ENOMEM)?;
        let thread = Arc::new(Thread {
            id: NEXT_THREAD_ID.fetch_add(1, Ordering::Relaxed),
            name,
            process,
            state: AtomicU8::new(ThreadState::Ready as u8),
            priority,
            sched: SchedContext::fair(priority),
            kernel_stack_base: base,
            kernel_stack_top: top,
            saved_rsp: core::cell::UnsafeCell::new(0),
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
            wake_status: AtomicI64::new(0),
            pending_wake: AtomicBool::new(false),
            idle,
            last_cpu: AtomicU32::new(0),
            pinned: AtomicBool::new(false),
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
    ) -> Result<Arc<Thread>, Errno> {
        let thread = Self::new(
            name,
            Arc::clone(&process),
            PRIORITY_DEFAULT,
            true,
            cr3,
            false,
        )?;
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
        Ok(thread)
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
        Ok(thread)
    }

    /// Create the in-place idle thread for the boot CPU.
    pub fn new_idle(process: Arc<Process>, current_rsp: u64, cr3: u64) -> Arc<Thread> {
        Arc::new(Thread {
            id: NEXT_THREAD_ID.fetch_add(1, Ordering::Relaxed),
            name: String::from("idle"),
            process,
            state: AtomicU8::new(ThreadState::Running as u8),
            priority: PRIORITY_IDLE,
            sched: SchedContext::fair(PRIORITY_IDLE),
            kernel_stack_base: 0,
            kernel_stack_top: current_rsp & !0xf,
            saved_rsp: core::cell::UnsafeCell::new(0),
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
            wake_status: AtomicI64::new(0),
            pending_wake: AtomicBool::new(false),
            idle: true,
            last_cpu: AtomicU32::new(0),
            pinned: AtomicBool::new(false),
        })
    }

    /// Create the idle thread for an application processor.
    ///
    /// The AP is already executing on the returned kernel stack when this object is published, so
    /// the stack is not prepared with [`crate::sched::context::prepare_stack`]: the first
    /// `sched_switch` saves the live stack pointer.
    pub fn new_idle_ap(process: Arc<Process>, cr3: u64) -> Result<Arc<Thread>, Errno> {
        let (base, top) = allocate_kernel_stack().ok_or(Errno::ENOMEM)?;
        Ok(Arc::new(Thread {
            id: NEXT_THREAD_ID.fetch_add(1, Ordering::Relaxed),
            name: String::from("idle-ap"),
            process,
            state: AtomicU8::new(ThreadState::Running as u8),
            priority: PRIORITY_IDLE,
            sched: SchedContext::fair(PRIORITY_IDLE),
            kernel_stack_base: base,
            kernel_stack_top: top,
            saved_rsp: core::cell::UnsafeCell::new(0),
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
            wake_status: AtomicI64::new(0),
            pending_wake: AtomicBool::new(false),
            idle: true,
            last_cpu: AtomicU32::new(0),
            pinned: AtomicBool::new(false),
        }))
    }
}

/// Allocate a page-aligned kernel stack and unmap its lowest page as a guard.
///
/// Returns `(base, top)`, or `None` when the heap or the kernel mapper fails.
pub fn allocate_kernel_stack() -> Option<(u64, u64)> {
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
    Some((base, top))
}
