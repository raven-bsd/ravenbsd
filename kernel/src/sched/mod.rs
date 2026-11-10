// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Preemptive single-CPU scheduler.
//!
//! # Design
//!
//! Each thread has a 16 KiB kernel stack (plus an unmapped guard page) and a saved kernel stack
//! pointer. `context::switch` saves the callee-saved registers and `rsp`, installs the incoming
//! thread's CR3 (when its process differs), TSS `rsp0` and per-CPU kernel stack, and returns on
//! the incoming stack. A new thread starts at `sched_thread_start`, which either calls its kernel
//! entry function or builds an `iretq` frame for ring 3.
//!
//! The run queue is a fixed-capacity 32-level FIFO with a bitmap (`raven-sched-core`), so the
//! timer interrupt can enqueue woken threads without allocating. The sleep list is a deadline
//! sorted `Vec` pre-reserved at init; the timer only removes entries. Time slices are 10 ticks at
//! `HZ = 1000` (10 ms). `sched::tick` runs from the timer ISR and sets `need_resched`; preemption
//! happens on return to ring 3 and at explicit blocking/yield points.
//!
//! # Lock order
//!
//! 1. process table (`Process.handles`, `Process.address_space`)
//! 2. scheduler critical section (run queue, current thread, sleep list)
//! 3. wait queues (futex buckets)
//! 4. sleep list
//!
//! A wait-queue lock is never held while taking the run queue: `WaitQueue::wake` removes victims
//! under the bucket lock, releases it, then calls [`wake`]. The scheduler critical section may be
//! live across a context switch; that is the documented run-queue handoff, and every guard is
//! restored on the thread's own stack when it is scheduled again.
//!
//! No allocation happens in the timer ISR: wakeups push pre-existing `Arc` values and the sleep
//! list only removes entries.

pub mod context;
pub mod idle;
pub mod runqueue;
pub mod selftest;
pub mod sleep;
pub mod thread;
pub mod wait;

use core::cell::UnsafeCell;
use core::mem::MaybeUninit;
use core::sync::atomic::{AtomicBool, Ordering};

use alloc::format;
use alloc::string::String;
use alloc::sync::Arc;

use raven_abi::errno::Errno;

use crate::kprintln;
use crate::process::Process;
use crate::sync;
use crate::time;

use self::runqueue::{ThreadQueue, ThreadSleepList};
use self::thread::{PRIORITY_DEFAULT, Thread, ThreadState};

/// Timer ticks in one scheduling slice (`HZ = 1000`, so 10 ms).
pub const SLICE_TICKS: u32 = 10;
/// Sleep-list capacity reserved at init, so the syscall path never allocates under the lock.
const SLEEP_CAPACITY: usize = 128;

struct SchedState {
    runqueue: ThreadQueue,
    sleepers: ThreadSleepList,
    current: Option<Arc<Thread>>,
    idle: Arc<Thread>,
    kernel_process: Arc<Process>,
    kernel_cr3: u64,
    need_resched: bool,
    switches: u64,
    next_token: u64,
}

struct StateCell(UnsafeCell<MaybeUninit<SchedState>>);

// SAFETY: the cell is written once by `init` before `INITIALIZED` is published and is only
// accessed from scheduler critical sections afterwards.
unsafe impl Sync for StateCell {}

static SCHED: StateCell = StateCell(UnsafeCell::new(MaybeUninit::uninit()));
static INITIALIZED: AtomicBool = AtomicBool::new(false);

/// Initialise the scheduler and adopt the current boot context as the idle thread.
///
/// Idempotent; safe to call from both the self-test and the `/init` paths.
pub fn init() -> Result<(), Errno> {
    if INITIALIZED.load(Ordering::Acquire) {
        return Ok(());
    }
    if !context::fpu_state_ok() {
        return Err(Errno::EINVAL);
    }
    let kernel_process = crate::process::new_kernel_process()?;
    let kernel_cr3 = context::read_cr3();
    let idle = Thread::new_idle(Arc::clone(&kernel_process), context::read_rsp(), kernel_cr3);
    let state = SchedState {
        runqueue: ThreadQueue::new(),
        sleepers: ThreadSleepList::with_capacity(SLEEP_CAPACITY),
        current: Some(Arc::clone(&idle)),
        idle,
        kernel_process,
        kernel_cr3,
        need_resched: false,
        switches: 0,
        next_token: 1,
    };
    // SAFETY: the scheduler has not been published yet; this is the only writer.
    unsafe {
        MaybeUninit::write(&mut *SCHED.0.get(), state);
    }
    INITIALIZED.store(true, Ordering::Release);
    Ok(())
}

/// Address of the current thread's `Thread` object.
///
/// # Safety
///
/// The caller must hold a scheduler critical section, or otherwise guarantee that the current
/// thread cannot exit concurrently.
unsafe fn state() -> &'static mut SchedState {
    // SAFETY: `init` wrote the value before publishing `INITIALIZED`.
    unsafe { (*SCHED.0.get()).assume_init_mut() }
}

/// The current thread, if the scheduler has started.
pub fn current_thread() -> Option<Arc<Thread>> {
    if !INITIALIZED.load(Ordering::Acquire) {
        return None;
    }
    let _guard = sync::critical();
    // SAFETY: initialised and inside the scheduler critical section.
    unsafe { state().current.clone() }
}

/// The process owning the current thread.
pub fn current_process() -> Option<Arc<Process>> {
    current_thread().map(|thread| Arc::clone(&thread.process))
}

/// Put a ready thread on its priority FIFO; false when the queue is full.
pub fn enqueue(thread: Arc<Thread>) -> bool {
    if !INITIALIZED.load(Ordering::Acquire) {
        return false;
    }
    let _guard = sync::critical();
    // SAFETY: initialised and inside the scheduler critical section.
    let state = unsafe { state() };
    if state
        .runqueue
        .push_back(thread.priority, Arc::clone(&thread))
        .is_err()
    {
        return false;
    }
    thread.set_state(ThreadState::Ready);
    thread.process.thread_started();
    state.need_resched = true;
    true
}

/// Spawn a kernel thread at `entry`.
pub fn spawn_kernel(name: &str, entry: fn()) -> Result<Arc<Thread>, Errno> {
    let (process, cr3) = {
        if !INITIALIZED.load(Ordering::Acquire) {
            return Err(Errno::EINVAL);
        }
        let _guard = sync::critical();
        // SAFETY: initialised and inside the scheduler critical section.
        let state = unsafe { state() };
        (Arc::clone(&state.kernel_process), state.kernel_cr3)
    };
    let thread = Thread::new_kernel(String::from(name), process, entry, cr3, PRIORITY_DEFAULT)?;
    if !enqueue(Arc::clone(&thread)) {
        return Err(Errno::EAGAIN);
    }
    Ok(thread)
}

/// Create a ring-3 thread in `process`, without enqueueing it.
pub fn new_user_thread(
    process: &Arc<Process>,
    entry: u64,
    stack_top: u64,
    arg: u64,
) -> Result<Arc<Thread>, Errno> {
    let cr3 = process
        .address_space
        .lock()
        .as_ref()
        .map(|address_space| address_space.root().0)
        .ok_or(Errno::EINVAL)?;
    let name = format!("{}-thread", process.id);
    Thread::new_user(name, Arc::clone(process), entry, stack_top, arg, cr3)
}

/// Start the scheduler with `thread` as the first runnable user thread.
pub fn start_user(thread: Arc<Thread>) -> ! {
    if !enqueue(thread) {
        kprintln!("sched: initial thread queue is full");
        crate::boot::exit_qemu(0x11);
    }
    start()
}

/// Become the idle thread and run the best ready thread.
pub fn start() -> ! {
    if !INITIALIZED.load(Ordering::Acquire) {
        kprintln!("sched: not initialised");
        crate::boot::exit_qemu(0x11);
    }
    kprintln!("sched: running (1 cpu, slice 10 ms)");
    {
        let _guard = sync::critical();
        schedule_locked();
    }
    idle::run()
}

/// Pick the next thread and switch to it. Must be called inside a critical section.
///
/// On return the CPU runs the caller again; the caller's guard is still live and restores its
/// saved interrupt state when dropped.
fn schedule_locked() {
    // SAFETY: the caller holds the scheduler critical section.
    let current = match unsafe { state() }.current.clone() {
        Some(thread) => thread,
        None => return,
    };

    let requeue = !current.idle && current.state() == ThreadState::Running;
    let next = {
        // SAFETY: same critical section; the borrow ends before the context switch.
        let state = unsafe { state() };
        if requeue {
            current.set_state(ThreadState::Ready);
            let _ = state
                .runqueue
                .push_back(current.priority, Arc::clone(&current));
        }
        state.need_resched = false;
        let next = match state.runqueue.pop_front() {
            Some(next) => next,
            None => Arc::clone(&state.idle),
        };
        if Arc::ptr_eq(&next, &current) {
            current.set_state(ThreadState::Running);
            return;
        }
        next.set_state(ThreadState::Running);
        next.slice_ticks.store(SLICE_TICKS, Ordering::Relaxed);
        state.switches = state.switches.saturating_add(1);
        state.current = Some(Arc::clone(&next));
        next
    };

    // SAFETY: critical section held; both `Arc`s keep the threads alive for the switch.
    unsafe {
        context::switch(&current, &next);
    }
}

/// Timer tick: wake expired sleepers and account the current thread's slice.
pub fn tick() {
    if !INITIALIZED.load(Ordering::Acquire) {
        return;
    }
    let _guard = sync::critical();
    // SAFETY: initialised and inside the scheduler critical section.
    let state = unsafe { state() };
    let now = time::now_ns();
    while let Some((thread, token, status)) = state.sleepers.pop_expired(now) {
        wake_locked(state, &thread, Some(token), status);
    }
    let current = match state.current.clone() {
        Some(current) => current,
        None => return,
    };
    let remaining = current.slice_ticks.load(Ordering::Relaxed);
    if remaining > 1 {
        current
            .slice_ticks
            .store(remaining.saturating_sub(1), Ordering::Relaxed);
    } else {
        current.slice_ticks.store(SLICE_TICKS, Ordering::Relaxed);
        if !state.runqueue.is_empty() {
            state.need_resched = true;
        }
    }
}

/// Return-from-interrupt hook called by the timer stub.
pub fn timer_return(from_user: bool) {
    if !from_user || !sync::preemption_enabled() {
        return;
    }
    preempt_if_needed();
}

/// Return-to-user hook called after a syscall.
pub fn syscall_return() {
    if !sync::preemption_enabled() {
        return;
    }
    preempt_if_needed();
}

fn preempt_if_needed() {
    if !need_resched() {
        return;
    }
    let _guard = sync::critical();
    schedule_locked();
}

/// True when a reschedule was requested.
pub fn need_resched() -> bool {
    if !INITIALIZED.load(Ordering::Acquire) {
        return false;
    }
    let _guard = sync::critical();
    // SAFETY: initialised and inside the scheduler critical section.
    unsafe { state().need_resched }
}

/// `YIELD`: move to the back of the priority FIFO.
pub fn yield_now() {
    if !INITIALIZED.load(Ordering::Acquire) {
        return;
    }
    let _guard = sync::critical();
    schedule_locked();
}

/// Sleep until the monotonic `deadline`; returns 0 or `-ETIMEDOUT`.
pub fn sleep_until(deadline: u64) -> isize {
    if !INITIALIZED.load(Ordering::Acquire) {
        return 0;
    }
    let _guard = sync::critical();
    // SAFETY: initialised and inside the scheduler critical section.
    let state = unsafe { state() };
    if deadline <= time::now_ns() {
        return 0;
    }
    let current = match state.current.clone() {
        Some(current) if !current.idle => current,
        _ => return 0,
    };
    let token = state.next_token;
    state.next_token = token.wrapping_add(1);
    if token == 0 {
        state.next_token = 1;
    }
    current.block_token.store(token, Ordering::SeqCst);
    current.pending_wake.store(false, Ordering::SeqCst);
    current.wake_status.store(0, Ordering::SeqCst);
    state
        .sleepers
        .insert(deadline, (Arc::clone(&current), token, 0));
    current.set_state(ThreadState::Blocked);
    schedule_locked();
    current.wake_status.load(Ordering::SeqCst) as isize
}

/// Start a blocking operation and return its wake token.
///
/// The caller then optionally registers a timeout with [`add_sleep`], publishes a waiter, and
/// calls [`block_current`]. [`end_block`] clears the token and any stale timeout entry.
pub fn begin_block() -> Result<u64, Errno> {
    if !INITIALIZED.load(Ordering::Acquire) {
        return Err(Errno::EINVAL);
    }
    let _guard = sync::critical();
    // SAFETY: initialised and inside the scheduler critical section.
    let state = unsafe { state() };
    let current = state.current.clone().ok_or(Errno::ESRCH)?;
    let token = state.next_token;
    state.next_token = token.wrapping_add(1);
    if token == 0 {
        state.next_token = 1;
    }
    current.block_token.store(token, Ordering::SeqCst);
    current.pending_wake.store(false, Ordering::SeqCst);
    current.wake_status.store(0, Ordering::SeqCst);
    Ok(token)
}

/// Register a timeout for the current blocking operation with its wake status.
pub fn add_sleep(token: u64, deadline: u64, status: isize) {
    if !INITIALIZED.load(Ordering::Acquire) {
        return;
    }
    let _guard = sync::critical();
    // SAFETY: initialised and inside the scheduler critical section.
    let state = unsafe { state() };
    let current = match state.current.clone() {
        Some(current) => current,
        None => return,
    };
    if current.block_token.load(Ordering::SeqCst) != token {
        return;
    }
    state
        .sleepers
        .insert(deadline, (Arc::clone(&current), token, status));
}

/// Block the current thread until a wakeup arrives.
///
/// A wakeup that raced with the caller's setup is consumed here. Returns the wake status (0 or
/// `-errno`).
pub fn block_current() -> isize {
    if !INITIALIZED.load(Ordering::Acquire) {
        return 0;
    }
    let _guard = sync::critical();
    // SAFETY: initialised and inside the scheduler critical section.
    let state = unsafe { state() };
    let current = match state.current.clone() {
        Some(current) => current,
        None => return 0,
    };
    if current.pending_wake.swap(false, Ordering::SeqCst) {
        current.set_state(ThreadState::Running);
        return current.wake_status.load(Ordering::SeqCst) as isize;
    }
    current.set_state(ThreadState::Blocked);
    schedule_locked();
    current.wake_status.load(Ordering::SeqCst) as isize
}

/// Finish a blocking operation: drop the token and any timeout that did not fire.
pub fn end_block(token: u64) {
    if !INITIALIZED.load(Ordering::Acquire) {
        return;
    }
    let _guard = sync::critical();
    // SAFETY: initialised and inside the scheduler critical section.
    let state = unsafe { state() };
    if let Some(current) = state.current.clone()
        && current.block_token.load(Ordering::SeqCst) == token
    {
        current.block_token.store(0, Ordering::SeqCst);
        current.pending_wake.store(false, Ordering::SeqCst);
    }
    let _ = state
        .sleepers
        .remove_where(|(_, entry_token, _)| *entry_token == token);
}

/// Wake `thread`; `token` restricts the wakeup to the thread's current blocking operation.
pub fn wake(thread: Arc<Thread>, token: Option<u64>, status: isize) -> bool {
    if !INITIALIZED.load(Ordering::Acquire) {
        return false;
    }
    let _guard = sync::critical();
    // SAFETY: initialised and inside the scheduler critical section.
    let state = unsafe { state() };
    wake_locked(state, &thread, token, status)
}

fn wake_locked(
    state: &mut SchedState,
    thread: &Arc<Thread>,
    token: Option<u64>,
    status: isize,
) -> bool {
    if let Some(token) = token
        && thread.block_token.load(Ordering::SeqCst) != token
    {
        return false;
    }
    match thread.state() {
        ThreadState::Blocked => {
            thread.set_state(ThreadState::Ready);
            thread.wake_status.store(status as i64, Ordering::SeqCst);
            thread.pending_wake.store(false, Ordering::SeqCst);
            if state
                .runqueue
                .push_back(thread.priority, Arc::clone(thread))
                .is_err()
            {
                thread.set_state(ThreadState::Dead);
                return false;
            }
            state.need_resched = true;
            true
        }
        ThreadState::Running => {
            // The waiter has not reached `block_current` yet; the flag makes it return without
            // sleeping. The first wakeup wins the status.
            if !thread.pending_wake.swap(true, Ordering::SeqCst) {
                thread.wake_status.store(status as i64, Ordering::SeqCst);
            }
            true
        }
        ThreadState::Ready | ThreadState::Dead => false,
    }
}

/// Terminate the calling thread. The last thread of a process terminates the process.
pub fn exit_thread(code: i32) -> ! {
    let thread = match current_thread() {
        Some(thread) => thread,
        None => crate::arch::halt(),
    };
    thread.exit_code.store(code, Ordering::SeqCst);
    if thread.process.thread_exited() {
        crate::process::notify_exit(code);
    }
    let _guard = sync::critical();
    let next = {
        // SAFETY: initialised and inside the scheduler critical section.
        let state = unsafe { state() };
        thread.set_state(ThreadState::Dead);
        let next = match state.runqueue.pop_front() {
            Some(next) => next,
            None => Arc::clone(&state.idle),
        };
        next.set_state(ThreadState::Running);
        next.slice_ticks.store(SLICE_TICKS, Ordering::Relaxed);
        state.switches = state.switches.saturating_add(1);
        state.current = Some(Arc::clone(&next));
        next
    };
    // SAFETY: the critical section stays live on the dead thread's stack forever; that stack is
    // never freed, so the frame and the guard remain valid but unused.
    unsafe {
        context::switch(&thread, &next);
    }
    loop {
        core::hint::spin_loop();
    }
}

/// Number of successful context switches.
pub fn switch_count() -> u64 {
    if !INITIALIZED.load(Ordering::Acquire) {
        return 0;
    }
    let _guard = sync::critical();
    // SAFETY: initialised and inside the scheduler critical section.
    unsafe { state().switches }
}

const _: () = assert!(SLICE_TICKS == 10);
