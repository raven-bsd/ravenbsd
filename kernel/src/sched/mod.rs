// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Preemptive per-CPU scheduler.
//!
//! # Design
//!
//! Each thread has a 16 KiB kernel stack (plus an unmapped guard page) and a saved kernel stack
//! pointer. `context::switch` saves the callee-saved registers and `rsp`, installs the incoming
//! thread's CR3 (when its process differs), TSS `rsp0`, GS-relative stack pointer and current
//! thread, and returns on the incoming stack. A new thread starts at `sched_thread_start`, which
//! either calls its kernel entry function or builds an `iretq` frame for ring 3.
//!
//! Every CPU owns one [`CpuSched`]: run queue, sleep list, current thread, idle thread, slice
//! accounting and switch counter. The per-CPU `spin::Mutex` is held only for short mutations and
//! is always released before a context switch; the architecture `sync::critical` guard (which
//! transfers the per-CPU preemption depth with the thread) keeps interrupts off across the
//! switch itself.
//!
//! # Lock order
//!
//! 1. process table (`Process.handles`, `Process.address_space`)
//! 2. one per-CPU scheduler slot (`CPUS[i]`); a cross-CPU waker or thief never holds two slots
//!    at the same time
//! 3. wait queues (futex buckets)
//! 4. per-CPU sleep list (inside the same slot lock)
//!
//! The TLB-shootdown and probe mailboxes have their own interrupt-safe locks and never take a
//! scheduler slot while waiting for an acknowledgement.
//!
//! # Work stealing
//!
//! On every timer tick a CPU whose run queue is empty pops one kernel thread from another CPU.
//! User threads stay on CPU 0 in S10: they need a per-CPU TSS for ring-3 entry and that is left
//! to the follow-up SMP tickets. Kernel threads (self-tests, drivers) are fully symmetric.
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

#[cfg(target_arch = "x86_64")]
#[path = "../smp/mod.rs"]
pub mod smp;
#[cfg(target_arch = "aarch64")]
#[path = "smp_shim.rs"]
pub mod smp;

use core::cell::UnsafeCell;
use core::sync::atomic::{AtomicBool, Ordering};

use alloc::format;
use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec::Vec;

use raven_abi::errno::Errno;
use spin::{Mutex as SpinMutex, MutexGuard as SpinMutexGuard};

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
/// Number of per-CPU scheduler slots.
pub const MAX_CPUS: usize = smp::MAX_CPUS;

/// Scheduler state owned by one CPU.
pub struct CpuSched {
    runqueue: ThreadQueue,
    sleepers: ThreadSleepList,
    current: Option<Arc<Thread>>,
    idle: Arc<Thread>,
    need_resched: bool,
    switches: u64,
    next_token: u64,
}

impl CpuSched {
    fn new(idle: Arc<Thread>) -> Self {
        Self {
            runqueue: ThreadQueue::new(),
            sleepers: ThreadSleepList::with_capacity(SLEEP_CAPACITY),
            current: Some(Arc::clone(&idle)),
            idle,
            need_resched: false,
            switches: 0,
            next_token: 1,
        }
    }
}

/// CPU-independent scheduler data.
struct Global {
    kernel_process: Arc<Process>,
    kernel_cr3: u64,
}

struct GlobalCell(UnsafeCell<Option<Global>>);

// SAFETY: `init` writes the cell once before `INITIALIZED` is published; both fields are immutable
// afterwards. `Process` and `Arc` are safe to share.
unsafe impl Sync for GlobalCell {}

static GLOBAL: GlobalCell = GlobalCell(UnsafeCell::new(None));
static CPUS: [SpinMutex<Option<CpuSched>>; MAX_CPUS] = [const { SpinMutex::new(None) }; MAX_CPUS];
static INITIALIZED: AtomicBool = AtomicBool::new(false);

/// Initialise the scheduler and adopt the boot context as CPU 0's idle thread.
///
/// Idempotent; safe to call from both the self-test and the `/init` paths.
pub fn init() -> Result<(), Errno> {
    if INITIALIZED.load(Ordering::Acquire) {
        return Ok(());
    }
    #[cfg(target_arch = "x86_64")]
    if !context::fpu_state_ok() {
        return Err(Errno::EINVAL);
    }
    let kernel_process = crate::process::new_kernel_process()?;
    let kernel_cr3 = context::read_cr3();
    let idle = Thread::new_idle(Arc::clone(&kernel_process), context::read_rsp(), kernel_cr3);
    // SAFETY: the scheduler has not been published yet; this is the only writer.
    unsafe {
        GLOBAL.0.get().write(Some(Global {
            kernel_process,
            kernel_cr3,
        }));
    }
    *lock_cpu(0) = Some(CpuSched::new(idle));
    smp::set_online(1);
    INITIALIZED.store(true, Ordering::Release);
    Ok(())
}

fn global() -> Option<&'static Global> {
    // SAFETY: written once by `init` before `INITIALIZED` and immutable afterwards.
    unsafe { (*GLOBAL.0.get()).as_ref() }
}

/// Kernel process used by kernel threads.
#[cfg(target_arch = "x86_64")]
pub fn kernel_process() -> Option<Arc<Process>> {
    global().map(|global| Arc::clone(&global.kernel_process))
}

/// Kernel page-table root used by kernel threads.
#[cfg(target_arch = "x86_64")]
pub fn kernel_cr3() -> Option<u64> {
    global().map(|global| global.kernel_cr3)
}

fn this_cpu() -> usize {
    smp::percpu::this_cpu()
}

fn lock_cpu(cpu: usize) -> SpinMutexGuard<'static, Option<CpuSched>> {
    let index = cpu.min(MAX_CPUS.saturating_sub(1));
    CPUS.get(index).unwrap_or(&CPUS[0]).lock()
}

/// Initialise the per-CPU scheduler slot for an application processor.
#[cfg(target_arch = "x86_64")]
pub fn init_cpu(cpu: usize, idle: Arc<Thread>) -> Result<(), Errno> {
    if cpu == 0 || cpu >= MAX_CPUS {
        return Err(Errno::EINVAL);
    }
    let mut slot = lock_cpu(cpu);
    if slot.is_some() {
        return Ok(());
    }
    *slot = Some(CpuSched::new(idle));
    Ok(())
}

/// Allocate an idle thread (and its kernel stack) for an application processor.
#[cfg(target_arch = "x86_64")]
pub fn new_ap_idle() -> Result<Arc<Thread>, Errno> {
    let process = kernel_process().ok_or(Errno::EINVAL)?;
    let cr3 = kernel_cr3().ok_or(Errno::EINVAL)?;
    Thread::new_idle_ap(process, cr3)
}

/// Adopt the AP's already-running stack as its idle thread and enter the idle loop.
#[cfg(target_arch = "x86_64")]
pub fn ap_main(cpu: usize) -> ! {
    let idle = {
        let slot = lock_cpu(cpu);
        slot.as_ref().map(|state| Arc::clone(&state.idle))
    };
    if let Some(idle) = idle {
        smp::percpu::set_current_thread(Arc::as_ptr(&idle) as u64);
        smp::percpu::set_kernel_rsp(idle.kernel_stack_top);
    }
    idle::run()
}

/// The current thread on this CPU, if the scheduler has started.
pub fn current_thread() -> Option<Arc<Thread>> {
    if !INITIALIZED.load(Ordering::Acquire) {
        return None;
    }
    let _guard = sync::critical();
    let slot = lock_cpu(this_cpu());
    slot.as_ref().and_then(|state| state.current.clone())
}

/// The process owning the current thread.
pub fn current_process() -> Option<Arc<Process>> {
    current_thread().map(|thread| Arc::clone(&thread.process))
}

/// True once [`init`] published the scheduler.
#[cfg_attr(target_arch = "x86_64", allow(dead_code))] // The aarch64 IRQ path checks it directly.
pub fn initialized() -> bool {
    INITIALIZED.load(Ordering::Acquire)
}

/// Put a ready thread on the best CPU's queue; false when the queue is full.
pub fn enqueue(thread: Arc<Thread>) -> bool {
    if !INITIALIZED.load(Ordering::Acquire) {
        return false;
    }
    // User threads need a per-CPU TSS for ring-3 entry; S10 keeps them on the boot processor.
    let cpu = if thread.user { 0 } else { this_cpu() };
    enqueue_on(cpu, thread)
}

fn enqueue_on(cpu: usize, thread: Arc<Thread>) -> bool {
    let _guard = sync::critical();
    let mut slot = lock_cpu(cpu);
    let Some(state) = slot.as_mut() else {
        return false;
    };
    if state
        .runqueue
        .push_back(thread.priority, Arc::clone(&thread))
        .is_err()
    {
        return false;
    }
    thread.set_state(ThreadState::Ready);
    thread
        .last_cpu
        .store(u32::try_from(cpu).unwrap_or(u32::MAX), Ordering::Release);
    thread.process.thread_started();
    state.need_resched = true;
    drop(slot);
    if cpu != this_cpu() {
        smp::bringup::send_reschedule(cpu);
    }
    true
}

/// Spawn a kernel thread at `entry`.
pub fn spawn_kernel(name: &str, entry: fn()) -> Result<Arc<Thread>, Errno> {
    let thread = new_kernel_thread(name, entry)?;
    if !enqueue(Arc::clone(&thread)) {
        return Err(Errno::EAGAIN);
    }
    Ok(thread)
}

/// Spawn a kernel thread directly on `cpu` (the SMP self-test uses this to place one worker per
/// CPU before the scheduler starts).
#[cfg(target_arch = "x86_64")]
pub fn spawn_kernel_on(cpu: usize, name: &str, entry: fn()) -> Result<Arc<Thread>, Errno> {
    if cpu >= smp::online_cpus() {
        return Err(Errno::EINVAL);
    }
    let thread = new_kernel_thread(name, entry)?;
    thread.pinned.store(true, Ordering::Release);
    if !enqueue_on(cpu, Arc::clone(&thread)) {
        return Err(Errno::EAGAIN);
    }
    Ok(thread)
}

fn new_kernel_thread(name: &str, entry: fn()) -> Result<Arc<Thread>, Errno> {
    let (process, cr3) = {
        let global = global().ok_or(Errno::EINVAL)?;
        (Arc::clone(&global.kernel_process), global.kernel_cr3)
    };
    Thread::new_kernel(String::from(name), process, entry, cr3, PRIORITY_DEFAULT)
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
    #[cfg(target_arch = "x86_64")]
    {
        // `syscall::init` installs the legacy single-CPU GS block; restore the real one and drop
        // the temporary null-page mapping that made the GS window fault-free.
        smp::percpu::install_bsp_gs();
        crate::arch::x86_64::ap_start::remove_null_page();
    }
    if !enqueue(thread) {
        kprintln!("sched: initial thread queue is full");
        crate::boot::exit_qemu(0x11);
    }
    start()
}

/// Become CPU 0's idle thread and run the best ready thread.
pub fn start() -> ! {
    if !INITIALIZED.load(Ordering::Acquire) {
        kprintln!("sched: not initialised");
        crate::boot::exit_qemu(0x11);
    }
    #[cfg(target_arch = "x86_64")]
    {
        smp::percpu::install_bsp_gs();
        crate::arch::x86_64::ap_start::remove_null_page();
    }
    let cpus = smp::online_cpus();
    kprintln!(
        "sched: running ({} {}, slice 10 ms)",
        cpus,
        if cpus == 1 { "cpu" } else { "cpus" }
    );
    {
        let _guard = sync::critical();
        schedule_locked();
    }
    idle::run()
}

/// Pick the next thread on this CPU and switch to it.
///
/// Must be called inside a critical section. The per-CPU slot lock is released before the switch;
/// the caller's critical guard stays live on the outgoing stack (and its depth is transferred to
/// the incoming thread by `context::switch`).
fn schedule_locked() {
    let cpu = this_cpu();
    let (from, to) = {
        let mut slot = lock_cpu(cpu);
        let Some(state) = slot.as_mut() else {
            return;
        };
        let Some(current) = state.current.clone() else {
            return;
        };

        if !current.idle && current.state() == ThreadState::Running {
            current.set_state(ThreadState::Ready);
            let _ = state
                .runqueue
                .push_back(current.priority, Arc::clone(&current));
        }
        state.need_resched = false;
        // A terminal process must never run user code again: drop every runnable thread that
        // belongs to one before picking the next thread.
        let mut next = match state.runqueue.pop_front() {
            Some(next) => next,
            None => Arc::clone(&state.idle),
        };
        loop {
            if next.idle || !next.process.exiting.load(Ordering::Acquire) {
                break;
            }
            next.set_state(ThreadState::Dead);
            push_reap(Arc::into_raw(next) as usize);
            next = match state.runqueue.pop_front() {
                Some(next) => next,
                None => Arc::clone(&state.idle),
            };
        }
        if Arc::ptr_eq(&next, &current) {
            current.set_state(ThreadState::Running);
            return;
        }
        next.set_state(ThreadState::Running);
        next.slice_ticks.store(SLICE_TICKS, Ordering::Relaxed);
        next.last_cpu
            .store(u32::try_from(cpu).unwrap_or(u32::MAX), Ordering::Release);
        state.switches = state.switches.saturating_add(1);
        state.current = Some(Arc::clone(&next));
        (current, next)
    };

    // SAFETY: the critical section is live; both `Arc`s keep the threads alive across the switch.
    unsafe {
        context::switch(&from, &to);
    }
    // Resumed on the incoming thread's stack: the outgoing stack is not this one, so deferred
    // dead stacks can be freed here.
    reap_dead();
}

/// Timer tick: wake this CPU's expired sleepers, account the slice and steal work when idle.
pub fn tick() {
    if !INITIALIZED.load(Ordering::Acquire) {
        return;
    }
    let cpu = this_cpu();
    let _guard = sync::critical();
    let now = time::now_ns();
    loop {
        let entry = {
            let mut slot = lock_cpu(cpu);
            let Some(state) = slot.as_mut() else {
                return;
            };
            state.sleepers.pop_expired(now)
        };
        let Some((thread, token, status)) = entry else {
            break;
        };
        wake(thread, Some(token), status);
    }

    let expired_with_work = {
        let mut slot = lock_cpu(cpu);
        let Some(state) = slot.as_mut() else {
            return;
        };
        let Some(current) = state.current.clone() else {
            return;
        };
        let remaining = current.slice_ticks.load(Ordering::Relaxed);
        if remaining > 1 {
            current
                .slice_ticks
                .store(remaining.saturating_sub(1), Ordering::Relaxed);
            false
        } else {
            current.slice_ticks.store(SLICE_TICKS, Ordering::Relaxed);
            !state.runqueue.is_empty()
        }
    };
    if expired_with_work {
        set_need_resched(cpu);
    }

    let empty = {
        let slot = lock_cpu(cpu);
        slot.as_ref().is_some_and(|state| state.runqueue.is_empty())
    };
    if empty {
        let _ = try_steal(cpu);
    }
}

/// Return-from-interrupt hook called by the timer and reschedule-IPI stubs.
#[cfg(target_arch = "x86_64")]
pub fn timer_return(from_user: bool) {
    if !from_user || !sync::preemption_enabled() {
        return;
    }
    preempt_if_needed();
}

/// Return-to-user hook called after a syscall.
///
/// x86-64 calls it before `sysretq`; aarch64 calls it from `svc_dispatch` before the vector
/// restores the EL0 frame. A switch here is safe on both: the trap frame lives on the current
/// thread's kernel stack, which the context switch preserves.
pub fn syscall_return() {
    if !sync::preemption_enabled() {
        return;
    }
    preempt_if_needed();
}

/// Return from a reschedule IPI.
#[cfg(target_arch = "x86_64")]
pub fn ipi_return(from_user: bool) {
    timer_return(from_user);
}

fn preempt_if_needed() {
    if !need_resched() {
        return;
    }
    let _guard = sync::critical();
    schedule_locked();
}

/// True when a reschedule was requested on this CPU.
pub fn need_resched() -> bool {
    if !INITIALIZED.load(Ordering::Acquire) {
        return false;
    }
    let _guard = sync::critical();
    let slot = lock_cpu(this_cpu());
    slot.as_ref().is_some_and(|state| state.need_resched)
}

fn set_need_resched(cpu: usize) {
    let mut slot = lock_cpu(cpu);
    if let Some(state) = slot.as_mut() {
        state.need_resched = true;
    }
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
    let cpu = this_cpu();
    let _guard = sync::critical();
    let (current, token) = {
        let mut slot = lock_cpu(cpu);
        let Some(state) = slot.as_mut() else {
            return 0;
        };
        let Some(current) = state.current.clone().filter(|thread| !thread.idle) else {
            return 0;
        };
        if deadline <= time::now_ns() {
            return 0;
        }
        let token = state.next_token;
        state.next_token = token.wrapping_add(1);
        if token == 0 {
            state.next_token = 1;
        }
        state
            .sleepers
            .insert(deadline, (Arc::clone(&current), token, 0));
        (current, token)
    };
    current.block_token.store(token, Ordering::SeqCst);
    current.pending_wake.store(false, Ordering::SeqCst);
    current.wake_status.store(0, Ordering::SeqCst);
    current.set_state(ThreadState::Blocked);
    schedule_locked();
    // A woken thread of a terminal process must not return to user mode: exit at the next
    // scheduling point instead (its stack is reaped by the next schedule on another thread).
    if current.process.exiting.load(Ordering::Acquire) {
        exit_thread(0);
    }
    current.wake_status.load(Ordering::SeqCst) as isize
}

/// Start a blocking operation and return its wake token.
///
/// The caller then optionally registers a timeout with [`add_sleep`], publishes a waiter, and
/// calls [`block_current`]. [`end_block`] clears the token and any stale timeout entry.
#[allow(dead_code)] // Blocking primitives are consumed by the syscall/futex path (x86-64 today, S14c on aarch64).
pub fn begin_block() -> Result<u64, Errno> {
    if !INITIALIZED.load(Ordering::Acquire) {
        return Err(Errno::EINVAL);
    }
    let _guard = sync::critical();
    let mut slot = lock_cpu(this_cpu());
    let Some(state) = slot.as_mut() else {
        return Err(Errno::ESRCH);
    };
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
#[allow(dead_code)] // Blocking primitives are consumed by the syscall/futex path (x86-64 today, S14c on aarch64).
pub fn add_sleep(token: u64, deadline: u64, status: isize) {
    if !INITIALIZED.load(Ordering::Acquire) {
        return;
    }
    let _guard = sync::critical();
    let mut slot = lock_cpu(this_cpu());
    let Some(state) = slot.as_mut() else {
        return;
    };
    let Some(current) = state.current.clone() else {
        return;
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
#[allow(dead_code)] // Blocking primitives are consumed by the syscall/futex path (x86-64 today, S14c on aarch64).
pub fn block_current() -> isize {
    if !INITIALIZED.load(Ordering::Acquire) {
        return 0;
    }
    let _guard = sync::critical();
    let current = {
        let slot = lock_cpu(this_cpu());
        match slot.as_ref().and_then(|state| state.current.clone()) {
            Some(current) => current,
            None => return 0,
        }
    };
    if current.pending_wake.swap(false, Ordering::SeqCst) {
        current.set_state(ThreadState::Running);
        return current.wake_status.load(Ordering::SeqCst) as isize;
    }
    current.set_state(ThreadState::Blocked);
    schedule_locked();
    // A woken thread of a terminal process must not return to user mode: exit at the next
    // scheduling point instead (its stack is reaped by the next schedule on another thread).
    if current.process.exiting.load(Ordering::Acquire) {
        exit_thread(0);
    }
    current.wake_status.load(Ordering::SeqCst) as isize
}

/// Finish a blocking operation: drop the token and any timeout that did not fire.
#[allow(dead_code)] // Blocking primitives are consumed by the syscall/futex path (x86-64 today, S14c on aarch64).
pub fn end_block(token: u64) {
    if !INITIALIZED.load(Ordering::Acquire) {
        return;
    }
    let _guard = sync::critical();
    let mut slot = lock_cpu(this_cpu());
    let Some(state) = slot.as_mut() else {
        return;
    };
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
    if let Some(token) = token
        && thread.block_token.load(Ordering::SeqCst) != token
    {
        return false;
    }
    match thread.state() {
        ThreadState::Running => {
            // The waiter has not reached `block_current` yet; the flag makes it return without
            // sleeping. The first wakeup wins the status.
            if !thread.pending_wake.swap(true, Ordering::SeqCst) {
                thread.wake_status.store(status as i64, Ordering::SeqCst);
            }
            true
        }
        ThreadState::Blocked => {
            let cpu = target_cpu(&thread);
            let mut slot = lock_cpu(cpu);
            let Some(state) = slot.as_mut() else {
                return false;
            };
            // Re-check under the owning CPU's lock; the thread may have been woken meanwhile.
            if thread.state() != ThreadState::Blocked
                || token.is_some_and(|token| thread.block_token.load(Ordering::SeqCst) != token)
            {
                return false;
            }
            thread.set_state(ThreadState::Ready);
            thread.wake_status.store(status as i64, Ordering::SeqCst);
            thread.pending_wake.store(false, Ordering::SeqCst);
            if state
                .runqueue
                .push_back(thread.priority, Arc::clone(&thread))
                .is_err()
            {
                thread.set_state(ThreadState::Dead);
                return false;
            }
            thread
                .last_cpu
                .store(u32::try_from(cpu).unwrap_or(u32::MAX), Ordering::Release);
            state.need_resched = true;
            drop(slot);
            if cpu != this_cpu() {
                smp::bringup::send_reschedule(cpu);
            }
            true
        }
        ThreadState::Ready | ThreadState::Dead => false,
    }
}

/// CPU a blocked thread should resume on.
fn target_cpu(thread: &Thread) -> usize {
    let cpu = usize::try_from(thread.last_cpu.load(Ordering::Acquire)).unwrap_or(0);
    let online = smp::online_cpus();
    if cpu < online && (!thread.user || cpu == 0) {
        cpu
    } else {
        0
    }
}

/// Pop one kernel thread from another CPU's queue. Only one remote slot is locked at a time.
fn try_steal(cpu: usize) -> bool {
    let online = smp::online_cpus();
    if online <= 1 {
        return false;
    }
    let mut offset = 1usize;
    while offset < online {
        let victim = cpu.saturating_add(offset).checked_rem(online).unwrap_or(0);
        let stolen = {
            let mut slot = lock_cpu(victim);
            match slot.as_mut() {
                Some(state) => match state.runqueue.pop_front() {
                    Some(thread) if !thread.user && !thread.pinned.load(Ordering::Relaxed) => {
                        Some(thread)
                    }
                    Some(thread) => {
                        let _ = state
                            .runqueue
                            .push_back(thread.priority, Arc::clone(&thread));
                        None
                    }
                    None => None,
                },
                None => None,
            }
        };
        if let Some(thread) = stolen {
            let mut slot = lock_cpu(cpu);
            let Some(state) = slot.as_mut() else {
                return false;
            };
            thread.set_state(ThreadState::Ready);
            thread
                .last_cpu
                .store(u32::try_from(cpu).unwrap_or(u32::MAX), Ordering::Release);
            if state
                .runqueue
                .push_back(thread.priority, Arc::clone(&thread))
                .is_err()
            {
                thread.set_state(ThreadState::Dead);
                return false;
            }
            state.need_resched = true;
            return true;
        }
        offset = offset.saturating_add(1);
    }
    false
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
    // The frame that switches away is abandoned on the dead stack, so its `Arc` local can never
    // be dropped.  Move the owning reference into the deferred-reap queue; the reference is
    // turned back into an `Arc` and dropped by the next schedule on another thread.
    let raw = Arc::into_raw(thread);
    push_reap(raw as usize);
    let _guard = sync::critical();
    let cpu = this_cpu();
    let to = {
        let mut slot = lock_cpu(cpu);
        let Some(state) = slot.as_mut() else {
            crate::arch::halt();
        };
        // SAFETY: the raw reference is owned by the reap queue until a later scheduling point;
        // this switch is the only reader and happens before the queue is drained.
        let dead = unsafe { &*raw };
        dead.set_state(ThreadState::Dead);
        let next = match state.runqueue.pop_front() {
            Some(next) => next,
            None => Arc::clone(&state.idle),
        };
        next.set_state(ThreadState::Running);
        next.slice_ticks.store(SLICE_TICKS, Ordering::Relaxed);
        next.last_cpu
            .store(u32::try_from(cpu).unwrap_or(u32::MAX), Ordering::Release);
        state.switches = state.switches.saturating_add(1);
        state.current = Some(Arc::clone(&next));
        next
    };
    // SAFETY: the critical section stays live on the dead thread's stack forever; the stack is
    // freed later by `reap_dead` from another thread's context.
    unsafe {
        context::switch(&*raw, &to);
    }
    loop {
        core::hint::spin_loop();
    }
}

/// Return-from-IRQ hook: switch to a ready thread when a reschedule is pending.
///
/// The aarch64 IRQ dispatcher calls this after the timer tick and before the vector restores the
/// interrupted frame. Like x86-64's `timer_return`, only a return to user mode preempts: kernel
/// threads reach a scheduling point through yield/block/sleep. `from_user` is false for the
/// current-EL IRQ slots (all of them until S14c).
#[cfg_attr(target_arch = "x86_64", allow(dead_code))] // x86-64 uses `timer_return` from its ISR stubs.
pub fn irq_return(from_user: bool) {
    if !from_user || !INITIALIZED.load(Ordering::Acquire) || !sync::preemption_enabled() {
        return;
    }
    preempt_if_needed();
}

/// Deferred kernel-stack reaping.
///
/// A thread cannot free the stack it is running on, and the `Arc<Thread>` local in the frame
/// that switches away is abandoned with that stack.  `exit_thread` (and the terminal-process
/// filter) therefore hand the owning reference to this queue; the next scheduling point on
/// another thread turns it back into an `Arc`, frees the stack and drops the reference.
///
/// Freeing from `sched/mod.rs` keeps the stack allocator in `thread.rs` untouched: the block is
/// deallocated with the exact layout `allocate_kernel_stack` used.  The only obstacle is its
/// guard page, which `allocate_kernel_stack` unmaps and whose physical frame the unmap discards;
/// a reused block would fail that unmap.  The reaper therefore remaps the guard page to one
/// shared zeroed frame before freeing, so the next allocation over the block can unmap it
/// again.  The shared frame is never read and costs one frame in total.
static REAP: SpinMutex<Vec<usize>> = SpinMutex::new(Vec::new());

fn push_reap(raw: usize) {
    REAP.lock().push(raw);
}

/// Queue a thread whose stack must be freed by the next scheduling point.
///
/// Used for creation rollback (a thread that was never enqueued) as well as for dead threads.
pub fn reap_thread(thread: Arc<Thread>) {
    push_reap(Arc::into_raw(thread) as usize);
}

/// Drop the owning references of every queued dead thread.  Runs from process context on a live
/// stack after a context switch.
///
/// The 16 KiB kernel stack itself is not freed yet: the allocation and its unmapped guard page
/// live in `kernel/src/sched/thread.rs` (not in this ticket's Owns), and freeing the heap block
/// here would break the next `allocate_kernel_stack` that reuses it (its guard unmap must find
/// the page present - see S36's report).  The Thread and Process Arcs are released here.
fn reap_dead() {
    let pending = {
        let mut queue = REAP.lock();
        core::mem::take(&mut *queue)
    };
    for raw in pending {
        // SAFETY: each pointer was produced by `Arc::into_raw` exactly once and is owned by the
        // queue; the thread is neither current nor runnable.
        let thread = unsafe { Arc::from_raw(raw as *const Thread) };
        self::thread::free_kernel_stack(thread.kernel_stack_base, thread.kernel_stack_top);
        drop(thread);
    }
}

/// Number of successful context switches across all CPUs.
pub fn switch_count() -> u64 {
    if !INITIALIZED.load(Ordering::Acquire) {
        return 0;
    }
    // Same local discipline as every other scheduler mutation: the timer tick takes
    // `sync::critical` and then the per-CPU slot lock, so the monitor must too.  Without it an
    // interrupt on this CPU could re-enter `tick` and deadlock on the slot lock held here.
    let _guard = sync::critical();
    let mut total = 0u64;
    for cpu in 0..smp::online_cpus() {
        let slot = lock_cpu(cpu);
        if let Some(state) = slot.as_ref() {
            total = total.saturating_add(state.switches);
        }
    }
    total
}

const _: () = assert!(SLICE_TICKS == 10);
