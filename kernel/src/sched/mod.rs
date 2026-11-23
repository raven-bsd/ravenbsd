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
use core::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

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
/// `smp_user=on`: schedule user threads on application processors.  Off is the default until the
/// soak gate passes, and the off mode is exercised by `smp-user-off`.
static USER_SMP: AtomicBool = AtomicBool::new(false);
/// `smp-user-trace`: print the first user thread on each CPU and cross-CPU wakeups.
static USER_TRACE: AtomicBool = AtomicBool::new(false);
/// One-shot per-CPU marker for the first user thread (trace evidence only).
static USER_RAN: [AtomicBool; MAX_CPUS] = [const { AtomicBool::new(false) }; MAX_CPUS];

/// Read the `smp_user` and `smp-user-trace` boot flags once.
fn read_user_smp_flags() {
    let cmdline = crate::memory::paging::cmdline();
    for token in cmdline.split_ascii_whitespace() {
        if token == "smp_user=on" {
            USER_SMP.store(true, Ordering::Release);
        } else if token == "smp-user-trace" {
            USER_TRACE.store(true, Ordering::Release);
        }
    }
}

/// True when user threads may be scheduled on application processors.
pub fn user_smp_enabled() -> bool {
    USER_SMP.load(Ordering::Acquire)
}

/// True when the S75 evidence trace is enabled.
fn user_trace() -> bool {
    USER_TRACE.load(Ordering::Acquire)
}

/// Ask every online CPU except `excluding` to reschedule.
///
/// Used by process teardown so a user thread running on another CPU observes `process.exiting` at
/// its next scheduling point instead of continuing to run user code.
pub fn request_reschedule_all(excluding: usize) {
    let online = smp::online_cpus();
    let mut cpu = 0usize;
    while cpu < online {
        if cpu != excluding {
            set_need_resched(cpu);
            smp::bringup::send_reschedule(cpu);
        }
        cpu = cpu.saturating_add(1);
    }
}

/// Initialise the scheduler and adopt the boot context as CPU 0's idle thread.
///
/// Idempotent; safe to call from both the self-test and the `/init` paths.
pub fn init() -> Result<(), Errno> {
    if INITIALIZED.load(Ordering::Acquire) {
        return Ok(());
    }
    read_user_smp_flags();
    if !context::init_fpu() {
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
pub fn kernel_process() -> Option<Arc<Process>> {
    global().map(|global| Arc::clone(&global.kernel_process))
}

/// Kernel page-table root used by kernel threads.
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
pub fn new_ap_idle() -> Result<Arc<Thread>, Errno> {
    let process = kernel_process().ok_or(Errno::EINVAL)?;
    let cr3 = kernel_cr3().ok_or(Errno::EINVAL)?;
    Thread::new_idle_ap(process, cr3)
}

/// Adopt the AP's already-running stack as its idle thread and enter the idle loop.
pub fn ap_main(cpu: usize) -> ! {
    let idle = {
        let slot = lock_cpu(cpu);
        slot.as_ref().map(|state| Arc::clone(&state.idle))
    };
    if let Some(idle) = idle {
        smp::percpu::set_current_thread(Arc::as_ptr(&idle) as u64);
        smp::percpu::set_kernel_rsp(idle.kernel_stack_top);
    }
    #[cfg(target_arch = "aarch64")]
    {
        // FP/SIMD traps are per-CPU (`CPACR_EL1.FPEN`); enable them before this CPU may run a
        // user thread.
        let _ = context::init_fpu();
        smp::percpu::mark_online(cpu);
        // SAFETY: the AP now has its current idle thread and per-CPU slot installed; IRQs are
        // safe to take. The BSP waits for `mark_online` before it queues any AP work.
        unsafe {
            sync::enable_interrupts();
        }
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
    // User threads spread across the online CPUs when `smp_user=on`; otherwise they stay on the
    // boot processor. Both architectures now have a per-CPU EL0/syscall path and per-thread
    // FP/SIMD state, so aarch64 user threads follow the same policy. Kernel threads stay local.
    let cpu = if thread.user {
        if user_smp_enabled() {
            pick_user_cpu()
        } else {
            0
        }
    } else {
        this_cpu()
    };
    enqueue_on(cpu, thread)
}

/// Select-CPU policy for a new user thread: round-robin across the online CPUs.
///
/// Round-robin spreads the first threads of a process across every core, which is what the S75
/// soak measures. A load-aware policy can replace it once a real affinity API exists.
fn pick_user_cpu() -> usize {
    let online = smp::online_cpus();
    static NEXT_USER_CPU: AtomicUsize = AtomicUsize::new(0);
    let next = NEXT_USER_CPU.fetch_add(1, Ordering::Relaxed);
    next.checked_rem(online).unwrap_or(0)
}

fn enqueue_on(cpu: usize, thread: Arc<Thread>) -> bool {
    let _guard = sync::critical();
    let mut slot = lock_cpu(cpu);
    let Some(state) = slot.as_mut() else {
        return false;
    };
    if state
        .runqueue
        .push_back(thread.effective_priority(), Arc::clone(&thread))
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
    let priority = crate::process::priority::nice_to_priority(process.nice());
    let thread = Thread::new_user(
        name,
        Arc::clone(process),
        entry,
        stack_top,
        arg,
        cr3,
        priority,
    )?;
    process.register_thread(&thread);
    Ok(thread)
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
    let mut first_user: Option<(usize, u64, u32)> = None;
    let (from, to) = {
        let mut slot = lock_cpu(cpu);
        let Some(state) = slot.as_mut() else {
            return;
        };
        let Some(current) = state.current.clone() else {
            return;
        };

        if !current.idle && current.state() == ThreadState::Running {
            // K527: enqueue before publishing `Ready`. A runnable thread must own a queue slot;
            // under the MAX_THREADS bound this cannot fail. If it ever does, stay on the thread
            // in `Running` rather than dropping a runnable thread.
            if state
                .runqueue
                .push_back(current.effective_priority(), Arc::clone(&current))
                .is_err()
            {
                state.need_resched = false;
                return;
            }
            current.set_state(ThreadState::Ready);
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
            // The woken terminal-process thread never runs its own exit path, so remove its
            // timeout sleeper entry here as well as in `exit_thread`; otherwise the entry keeps
            // an `Arc<Thread>` until its deadline. The current CPU's list is under this slot;
            // the other CPUs are scanned without reacquiring it. The global futex queues are
            // also not process-owned: a wait entry enqueued after the process-exit sweep would
            // otherwise keep the dead Thread.
            let _ = state
                .sleepers
                .remove_where(|(thread, _, _)| thread.id == next.id);
            cancel_thread_sleepers_except(next.id, cpu);
            crate::syscall::futex::remove_thread(next.id);
            // This filter reaps a runnable terminal-process thread without running `exit_thread`,
            // so it must perform the same donation-graph teardown here. Without it the session's
            // remaining threads stay registered and the 3072-byte class still fills.
            crate::sync::unregister_thread(next.tid);
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
        let first_run = next.user
            && user_trace()
            && USER_RAN
                .get(cpu)
                .is_some_and(|flag| !flag.swap(true, Ordering::AcqRel));
        if first_run {
            first_user = Some((cpu, next.process.id, next.tid));
        }
        state.switches = state.switches.saturating_add(1);
        let to = Arc::as_ptr(&next);
        state.current = Some(next);
        (current, to)
    };

    if let Some((cpu, pid, tid)) = first_user {
        kprintln!("smp-user: cpu {cpu} first user thread pid={pid} tid={tid}");
    }
    // Park the outgoing self-reference in the Thread itself. A terminal-process filter can remove
    // this thread from the run queue and free its stack without ever resuming this frame; an
    // owning `Arc` local here would then be abandoned and keep the dead Thread alive forever. The
    // parked reference is taken back on resume, or dropped by `reap_dead` for a reaped thread.
    let from_raw = Arc::into_raw(from);
    let from_ptr = from_raw as *const Thread;
    // SAFETY: `from_raw` owns the Thread; while this frame is switched out no other context clears
    // this thread's `parked` field (the terminal filter hands the thread to the reaper, which is
    // the only other reader/writer).
    unsafe {
        (*from_ptr)
            .parked
            .store(from_raw as usize, Ordering::Release);
    }
    // SAFETY: the critical section is live. `from_ptr` is kept alive by its parked self-reference;
    // `to` is kept alive by `state.current` (the incoming thread is running and cannot be freed).
    unsafe {
        context::switch(&*from_ptr, &*to);
    }
    // Resumed: reclaim the self-reference parked above. A filter-reaped thread never reaches this
    // point; `reap_dead` takes the parked reference instead.
    let parked = unsafe { (*from_ptr).parked.swap(0, Ordering::AcqRel) };
    debug_assert_eq!(parked, from_raw as usize);
    // SAFETY: `parked` is the owning reference stored by this frame before the switch.
    drop(unsafe { Arc::from_raw(parked as *const Thread) });
    // Resumed on the incoming thread's stack: the outgoing stack is not this one, so deferred
    // dead stacks can be freed here.
    reap_dead();
}

/// Timer tick: wake this CPU's expired sleepers, account the slice and steal work when idle.
pub fn tick() {
    if !INITIALIZED.load(Ordering::Acquire) {
        return;
    }
    #[cfg(target_arch = "x86_64")]
    crate::sched::smp::percpu::note_irq();
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
/// Clear the process-level diagnostic token if it still names this thread's block.
///
/// A process can have several blocked threads; comparing before clearing keeps another
/// thread's more recent token intact. Single-threaded servers then read `wait_object != 0` as
/// "currently blocked" (K391 diagnostic).
fn note_unblock(current: &Thread) {
    current
        .process
        .wake_last_cpu
        .store(u32::MAX, Ordering::Relaxed);
    let token = current.block_token.load(Ordering::SeqCst);
    let _ = current.process.wait_object.compare_exchange(
        token,
        0,
        Ordering::Relaxed,
        Ordering::Relaxed,
    );
}

/// Serialise the `(state, pending_wake)` handshake against a concurrent wake.
///
/// Held only for the read-modify sequence, never across `schedule_locked`.
fn lock_block(thread: &Thread) -> BlockLockGuard<'_> {
    while thread.block_lock.swap(true, Ordering::Acquire) {
        core::hint::spin_loop();
    }
    BlockLockGuard(&thread.block_lock)
}

struct BlockLockGuard<'a>(&'a core::sync::atomic::AtomicBool);

impl Drop for BlockLockGuard<'_> {
    fn drop(&mut self) {
        self.0.store(false, Ordering::Release);
    }
}

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
    let block = lock_block(&current);
    current.block_token.store(token, Ordering::SeqCst);
    current.pending_wake.store(false, Ordering::SeqCst);
    current.wake_status.store(0, Ordering::SeqCst);
    if kick_pending(&current) {
        cancel_sleep(token);
        current.set_state(ThreadState::Running);
        return Errno::EINTR.to_ret();
    }
    current.interruptible.store(true, Ordering::SeqCst);
    current.set_state(ThreadState::Blocked);
    current.process.wait_object.store(
        current.block_token.load(Ordering::SeqCst),
        Ordering::Relaxed,
    );
    if current.pending_wake.swap(false, Ordering::SeqCst) {
        cancel_sleep(token);
        current.set_state(ThreadState::Running);
        note_unblock(&current);
        current.interruptible.store(false, Ordering::SeqCst);
        return current.wake_status.load(Ordering::SeqCst) as isize;
    }
    if kick_pending(&current) {
        cancel_sleep(token);
        current.set_state(ThreadState::Running);
        note_unblock(&current);
        current.interruptible.store(false, Ordering::SeqCst);
        return Errno::EINTR.to_ret();
    }
    drop(block);
    // The blocked frame must not own an `Arc<Thread>` across the switch. A terminal-process
    // filter can reap this thread without resuming it, and every Arc local left on the abandoned
    // stack can never be dropped. The scheduler keeps the thread alive through the parked
    // self-reference and the wait-queue/sleeper entries; re-fetch it after resuming.
    drop(current);
    schedule_locked();
    let Some(current) = current_thread() else {
        return 0;
    };
    // A woken thread of a terminal process must not return to user mode: exit at the next
    // scheduling point instead (its stack is reaped by the next schedule on another thread).
    if current.process.exiting.load(Ordering::Acquire) {
        exit_thread(0);
    }
    note_unblock(&current);
    current.interruptible.store(false, Ordering::SeqCst);
    if kick_pending(&current) {
        return Errno::EINTR.to_ret();
    }
    current.wake_status.load(Ordering::SeqCst) as isize
}

/// Remove a sleep-list entry that was registered but not consumed.
///
/// The caller already holds the scheduler critical section (`sleep_until`).
fn cancel_sleep(token: u64) {
    let mut slot = lock_cpu(this_cpu());
    if let Some(state) = slot.as_mut() {
        let _ = state
            .sleepers
            .remove_where(|(_, entry_token, _)| *entry_token == token);
    }
}

/// Remove an exiting thread from every per-CPU timeout sleeper list.
///
/// The donor removes a woken sleeper before the thread runs (NetBSD `sleepq_remove`,
/// FreeBSD `sleepq_abort`/`thread_unlink`). Raven defers that cleanup to the blocked caller's
/// `end_block`, but `block_current` and `sleep_until` exit a terminal-process thread before the
/// caller can run it, so the entry (which owns an `Arc<Thread>`) would stay until its deadline
/// and keep the dead Thread and its Process alive. Exit removes it explicitly; the scan is
/// bounded by the online CPU count and the queued sleeper count.
fn cancel_thread_sleepers(thread_id: u64) {
    let _guard = sync::critical();
    cancel_thread_sleepers_except(thread_id, usize::MAX);
}

/// Remove `thread_id` from every CPU's sleeper list except `skip`.
///
/// The caller either holds `skip`'s slot lock (the scheduler terminal filter removes that list
/// directly before calling here) or passes `usize::MAX` with the scheduler critical section held.
fn cancel_thread_sleepers_except(thread_id: u64, skip: usize) {
    for cpu in 0..smp::online_cpus() {
        if cpu == skip {
            continue;
        }
        let mut slot = lock_cpu(cpu);
        if let Some(state) = slot.as_mut() {
            let _ = state
                .sleepers
                .remove_where(|(thread, _, _)| thread.id == thread_id);
        }
    }
}

/// Start a blocking operation and return its wake token.
///
/// The caller then optionally registers a timeout with [`add_sleep`], publishes a waiter, and
/// calls [`block_current`]. [`end_block`] clears the token and any stale timeout entry.
#[allow(dead_code)] // Blocking primitives are consumed by the syscall/futex path (x86-64 today, S14c on aarch64).
pub fn begin_block() -> Result<u64, Errno> {
    begin_block_inner(false)
}

/// Start a blocking operation whose wait honours `THREAD_KICK`/`-EINTR`.
pub fn begin_block_interruptible() -> Result<u64, Errno> {
    begin_block_inner(true)
}

fn begin_block_inner(interruptible: bool) -> Result<u64, Errno> {
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
    current.interruptible.store(interruptible, Ordering::SeqCst);
    Ok(token)
}

/// True when the calling thread has a sticky kick pending.
///
/// A kick without a `SIG_SETUP` registration is consumed here and is not an interrupt: it must
/// not turn a child-exit wait (or any other unrelated wait) into `EINTR`. With a registration the
/// bits stay in the word so the return-to-user path can deliver them to the user entry point.
pub fn take_kick() -> bool {
    current_thread().is_some_and(|thread| kick_pending(&thread))
}

/// Test (and, without a registered handler, consume) one thread's kick word.
///
/// Only a registered `SIG_SETUP` entry can run a user handler, so only that case is a signal
/// interrupt. NetBSD `sleepq_block` has the same shape: `LW_CATCHINTR` plus a catchable pending
/// signal (`sigispending`/`issignal`), never a bare wake.
fn kick_pending(thread: &Thread) -> bool {
    if thread.kick_bits.load(Ordering::Acquire) == 0 {
        return false;
    }
    if thread.signal_entry.load(Ordering::Acquire) == 0 {
        thread.kick_bits.swap(0, Ordering::AcqRel);
        return false;
    }
    true
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
    let block = lock_block(&current);
    if current.pending_wake.swap(false, Ordering::SeqCst) {
        current.set_state(ThreadState::Running);
        current.interruptible.store(false, Ordering::SeqCst);
        return current.wake_status.load(Ordering::SeqCst) as isize;
    }
    if current.interruptible.load(Ordering::Acquire) && kick_pending(&current) {
        current.set_state(ThreadState::Running);
        current.interruptible.store(false, Ordering::SeqCst);
        return Errno::EINTR.to_ret();
    }
    current.set_state(ThreadState::Blocked);
    current.process.wait_object.store(
        current.block_token.load(Ordering::SeqCst),
        Ordering::Relaxed,
    );
    if current.pending_wake.swap(false, Ordering::SeqCst) {
        current.set_state(ThreadState::Running);
        note_unblock(&current);
        current.interruptible.store(false, Ordering::SeqCst);
        return current.wake_status.load(Ordering::SeqCst) as isize;
    }
    if current.interruptible.load(Ordering::Acquire) && kick_pending(&current) {
        current.set_state(ThreadState::Running);
        note_unblock(&current);
        current.interruptible.store(false, Ordering::SeqCst);
        return Errno::EINTR.to_ret();
    }
    drop(block);
    // The blocked frame must not own an `Arc<Thread>` across the switch. A terminal-process
    // filter can reap this thread without resuming it, and every Arc local left on the abandoned
    // stack can never be dropped. The scheduler keeps the thread alive through the parked
    // self-reference and the wait-queue/sleeper entries; re-fetch it after resuming.
    drop(current);
    schedule_locked();
    let Some(current) = current_thread() else {
        return 0;
    };
    // A woken thread of a terminal process must not return to user mode: exit at the next
    // scheduling point instead (its stack is reaped by the next schedule on another thread).
    if current.process.exiting.load(Ordering::Acquire) {
        exit_thread(0);
    }
    note_unblock(&current);
    current.interruptible.store(false, Ordering::SeqCst);
    if kick_pending(&current) {
        return Errno::EINTR.to_ret();
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
        current.interruptible.store(false, Ordering::SeqCst);
    }
    let _ = state
        .sleepers
        .remove_where(|(_, entry_token, _)| *entry_token == token);
}

/// Record why a wakeup could not enqueue its thread (K391 diagnostic).
fn note_wake_fail(thread: &Thread, reason: u64, cpu: usize) {
    let detail =
        (reason << 56) | ((cpu as u64 & 0x00ff_ffff) << 32) | (thread.state() as u64 & 0xff);
    thread
        .process
        .wake_fail_detail
        .store(detail, Ordering::Relaxed);
    thread.process.wake_fail.fetch_add(1, Ordering::Relaxed);
}

/// Wake `thread`; `token` restricts the wakeup to the thread's current blocking operation.
pub fn wake(thread: Arc<Thread>, token: Option<u64>, status: isize) -> bool {
    if !INITIALIZED.load(Ordering::Acquire) {
        return false;
    }
    let _guard = sync::critical();
    // Close the waiter/waker race: the target may be between its final `pending_wake` check
    // and `schedule_locked`. Holding the thread's block lock makes "read state" and "publish
    // the wake" atomic with the waiter's "set Blocked, re-check pending" sequence.
    let _block = lock_block(&thread);
    // A kick without a registered handler cannot run user code, so it is a plain wake, not an
    // EINTR. Keep the interrupt for a thread with a real SIG_SETUP entry.
    let status =
        if status == Errno::EINTR.to_ret() && thread.signal_entry.load(Ordering::Acquire) == 0 {
            0
        } else {
            status
        };
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
            thread.process.wake_pending.fetch_add(1, Ordering::Relaxed);
            true
        }
        ThreadState::Blocked => {
            let cpu = target_cpu(&thread);
            let mut slot = lock_cpu(cpu);
            let Some(state) = slot.as_mut() else {
                note_wake_fail(&thread, 1, cpu);
                return false;
            };
            // Re-check under the owning CPU's lock; the thread may have been woken meanwhile.
            if thread.state() != ThreadState::Blocked
                || token.is_some_and(|token| thread.block_token.load(Ordering::SeqCst) != token)
            {
                note_wake_fail(&thread, 2, cpu);
                return false;
            }
            // K527: enqueue first and only then publish `Ready`. When the queue refuses (it
            // cannot under the MAX_THREADS bound), leave the thread Blocked with the wake
            // recorded and let the caller retry; queue pressure is never a thread exit.
            thread.wake_status.store(status as i64, Ordering::SeqCst);
            if state
                .runqueue
                .push_back(thread.effective_priority(), Arc::clone(&thread))
                .is_err()
            {
                note_wake_fail(&thread, 3, cpu);
                thread.pending_wake.store(true, Ordering::SeqCst);
                return false;
            }
            thread.set_state(ThreadState::Ready);
            thread.pending_wake.store(false, Ordering::SeqCst);
            thread
                .last_cpu
                .store(u32::try_from(cpu).unwrap_or(u32::MAX), Ordering::Release);
            thread
                .process
                .wake_last_cpu
                .store(u32::try_from(cpu).unwrap_or(u32::MAX), Ordering::Relaxed);
            state.need_resched = true;
            drop(slot);
            if cpu != this_cpu() {
                if thread.user && user_trace() {
                    kprintln!("smp-user: wake tid {} -> cpu {}", thread.tid, cpu);
                }
                smp::bringup::send_reschedule(cpu);
            }
            true
        }
        ThreadState::Ready | ThreadState::Dead => false,
    }
}

/// Snapshot `(need_resched, runqueue_len)` for `cpu` (K391 diagnostic).
pub fn cpu_status(cpu: usize) -> (bool, usize) {
    let slot = lock_cpu(cpu);
    slot.as_ref().map_or((false, 0), |state| {
        (state.need_resched, state.runqueue.len())
    })
}

/// Total queued timeout sleepers across the online CPUs (S36 diagnostic).
pub fn sleepers_len() -> usize {
    let mut total = 0usize;
    for cpu in 0..smp::online_cpus() {
        let slot = lock_cpu(cpu);
        if let Some(state) = slot.as_ref() {
            total = total.saturating_add(state.sleepers.len());
        }
    }
    total
}

/// Total runnable threads across the online CPUs (S36 diagnostic).
pub fn runqueue_len() -> usize {
    let mut total = 0usize;
    for cpu in 0..smp::online_cpus() {
        let slot = lock_cpu(cpu);
        if let Some(state) = slot.as_ref() {
            total = total.saturating_add(state.runqueue.len());
        }
    }
    total
}

/// Current threads whose process is terminal (S36 diagnostic).
pub fn exiting_current() -> usize {
    let mut total = 0usize;
    for cpu in 0..smp::online_cpus() {
        let slot = lock_cpu(cpu);
        if let Some(state) = slot.as_ref() {
            if state
                .current
                .as_ref()
                .is_some_and(|thread| thread.process.exiting.load(Ordering::Acquire))
            {
                total = total.saturating_add(1);
            }
        }
    }
    total
}

/// CPU a blocked thread should resume on.
fn target_cpu(thread: &Thread) -> usize {
    let cpu = usize::try_from(thread.last_cpu.load(Ordering::Acquire)).unwrap_or(0);
    let online = smp::online_cpus();
    // A user thread resumes on the CPU it last ran on when user SMP is enabled; otherwise it
    // stays on the boot processor.
    let user_smp = thread.user && user_smp_enabled();
    if cpu < online && (!thread.user || user_smp) {
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
                            .push_back(thread.effective_priority(), Arc::clone(&thread));
                        None
                    }
                    None => None,
                },
                None => None,
            }
        };
        if let Some(thread) = stolen {
            // K527: queue membership is the only thing that makes the thread Ready. The victim
            // pop left it Ready but unqueued; only a successful destination push completes the
            // handoff.
            let pushed = {
                let mut slot = lock_cpu(cpu);
                match slot.as_mut() {
                    Some(state) => {
                        if state
                            .runqueue
                            .push_back(thread.effective_priority(), Arc::clone(&thread))
                            .is_ok()
                        {
                            state.need_resched = true;
                            true
                        } else {
                            false
                        }
                    }
                    None => false,
                }
            };
            if !pushed {
                // Destination full (impossible under the MAX_THREADS bound): restore the
                // thread to its victim queue; never mark a stolen thread Dead for pressure.
                let mut slot = lock_cpu(victim);
                if let Some(state) = slot.as_mut() {
                    let _ = state
                        .runqueue
                        .push_back(thread.effective_priority(), Arc::clone(&thread));
                }
                return false;
            }
            thread
                .last_cpu
                .store(u32::try_from(cpu).unwrap_or(u32::MAX), Ordering::Release);
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
    // FreeBSD `thread_unlink`/NetBSD `lwp_free` remove the thread from the process linkage at
    // exit. The live list is weak, but an unremoved entry is a permanent 16-byte leak.
    thread.process.unregister_thread(&thread);
    crate::syscall::futex::remove_thread(thread.id);
    thread.process.unregister_thread(&thread);
    // FreeBSD `thread_fini`/NetBSD `lwp_dtor`: a thread's turnstile registration does not outlive
    // the thread. The graph node holds base/effective map entries and donation edges; dropping it
    // here also restores any server or lock owner this thread was boosting.
    crate::sync::unregister_thread(thread.tid);
    // `block_current`/`sleep_until` exit a terminal-process thread before the caller runs
    // `end_block`, so the timeout entry (which holds an `Arc<Thread>`) must be removed here.
    cancel_thread_sleepers(thread.id);
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
        let to = Arc::as_ptr(&next);
        state.current = Some(next);
        to
    };
    // SAFETY: the critical section stays live on the dead thread's stack forever; the stack is
    // freed later by `reap_dead` from another thread's context. `to` is kept alive by
    // `state.current`; the dead stack owns no Arc to it.
    unsafe {
        context::switch(&*raw, &*to);
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
    // Kernel threads are cooperative: an IRQ that interrupts one must not switch away from it.
    // The `from_user` flag comes from the vector slot; this guard keeps a misclassified
    // current-EL slot from preempting a kernel worker.
    if !current_thread().is_some_and(|thread| thread.user) {
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

/// Threads waiting for the deferred stack reaper (S36 timeout diagnostic).
pub fn reap_len() -> usize {
    REAP.lock().len()
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
        // A terminal-process filter reaps a runnable thread without resuming the frame that
        // switched it out; the self-reference parked by `schedule_locked` is released here with
        // the reaper's own reference. Without this, the abandoned stack owns an Arc forever.
        let parked = thread.parked.swap(0, Ordering::AcqRel);
        if parked != 0 {
            // SAFETY: `schedule_locked` parked exactly one owning reference before switching out,
            // and this thread never resumed to reclaim it.
            drop(unsafe { Arc::from_raw(parked as *const Thread) });
        }
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
