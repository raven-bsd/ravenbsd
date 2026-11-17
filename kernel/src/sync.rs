// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Interrupt-safe locks and scheduler critical sections.
//!
//! Every lock here disables interrupts and increments the CPU's preemption counter, so the
//! scheduler cannot switch away while kernel state is being mutated. `IrqSpinLock` is a real
//! spinlock whose guard restores the saved interrupt state on drop. `critical()` is the same
//! critical section without a spin flag, used by the scheduler itself because the run queue is
//! per-CPU and is only mutated with interrupts off.
//!
//! x86-64 saves `RFLAGS.IF` and masks with `cli`; aarch64 saves `DAIF` and masks with
//! `daifset #0xf`. The preemption counter follows the running thread: `sched::context::switch`
//! stores the outgoing thread's counter and installs the incoming thread's value. A guard created
//! before a blocking context switch is therefore still live, and is dropped, when that thread
//! resumes.

use alloc::collections::BTreeMap;
use alloc::sync::Arc;
use core::cell::UnsafeCell;
use core::ops::{Deref, DerefMut};
use core::sync::atomic::{AtomicBool, AtomicU32, Ordering};

use crate::sched::smp::percpu;
use crate::sched::thread::Thread;

#[cfg(target_arch = "x86_64")]
const RFLAGS_IF: u64 = 1 << 9;

/// Disable interrupts and return the previous state.
#[cfg(target_arch = "x86_64")]
#[inline]
fn disable_interrupts() -> u64 {
    let flags: u64;
    // SAFETY: cli is ring-0 only and we run in the kernel.
    unsafe {
        core::arch::asm!(
            "pushfq",
            "pop {}",
            "cli",
            out(reg) flags,
            options(nomem, nostack),
        );
    }
    flags
}

/// Disable interrupts and return the previous state.
#[cfg(target_arch = "aarch64")]
#[inline]
fn disable_interrupts() -> u64 {
    let flags: u64;
    // SAFETY: DAIF is an EL1 system register; masking all asynchronous exceptions is the
    // architectural equivalent of `cli`.
    unsafe {
        core::arch::asm!(
            "mrs {}, daif",
            "msr daifset, #0xf",
            out(reg) flags,
            options(nomem, nostack, preserves_flags),
        );
    }
    flags
}

/// Restore the interrupt state captured by [`disable_interrupts`].
#[cfg(target_arch = "x86_64")]
#[inline]
fn restore_interrupts(flags: u64) {
    // SAFETY: sti/cli are ring-0 only; the caller passes flags captured by `disable_interrupts`.
    unsafe {
        if flags & RFLAGS_IF != 0 {
            core::arch::asm!("sti", options(nomem, nostack, preserves_flags));
        } else {
            core::arch::asm!("cli", options(nomem, nostack));
        }
    }
}

/// Restore the interrupt state captured by [`disable_interrupts`].
#[cfg(target_arch = "aarch64")]
#[inline]
fn restore_interrupts(flags: u64) {
    // SAFETY: DAIF is an EL1 system register; the value was captured by `disable_interrupts`.
    unsafe {
        core::arch::asm!("msr daif, {}", in(reg) flags, options(nomem, nostack));
    }
}

/// Current preemption nesting depth of the running thread on this CPU.
#[inline]
pub fn preempt_count() -> u32 {
    percpu::preempt_count()
}

/// Install a thread's saved preemption depth during a context switch.
///
/// # Safety
///
/// Must only be called from the context-switch path with interrupts disabled and with exclusive
/// access to the run queue.
#[inline]
pub(crate) unsafe fn set_preempt_count(value: u32) {
    percpu::set_preempt_count(value);
}

/// True when no lock or scheduler critical section is held.
#[inline]
pub fn preemption_enabled() -> bool {
    preempt_count() == 0
}

/// Enable interrupts.
///
/// # Safety
///
/// Only call once the interrupt vector table and the device gates are installed.
#[cfg(target_arch = "x86_64")]
#[inline]
pub unsafe fn enable_interrupts() {
    // SAFETY: the caller guarantees the IDT and gates are ready.
    unsafe {
        core::arch::asm!("sti", options(nomem, nostack, preserves_flags));
    }
}

/// Enable IRQs (FIQ/SError stay masked until a source exists).
///
/// # Safety
///
/// Only call once the vector table and the GIC/timer are initialised.
#[cfg(target_arch = "aarch64")]
#[inline]
pub unsafe fn enable_interrupts() {
    // SAFETY: the caller guarantees the vectors and the GIC/timer are ready.
    unsafe {
        core::arch::asm!("msr daifclr, #2", options(nomem, nostack, preserves_flags));
    }
}

/// A scheduler critical section guard.
///
/// Interrupts are off and `preempt_count` is incremented until the guard drops. The guard may be
/// live across a context switch: the incoming thread observes its own saved counter and the
/// outgoing thread's guard is restored when it is scheduled again. This is the documented
/// run-queue handoff.
pub struct CriticalGuard {
    flags: u64,
}

impl CriticalGuard {
    /// Enter the critical section.
    pub fn new() -> Self {
        let flags = disable_interrupts();
        percpu::preempt_inc();
        Self { flags }
    }
}

impl Default for CriticalGuard {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for CriticalGuard {
    fn drop(&mut self) {
        percpu::preempt_dec();
        restore_interrupts(self.flags);
    }
}

/// Enter a scheduler critical section: interrupts off, `preempt_count` incremented.
#[inline]
pub fn critical() -> CriticalGuard {
    CriticalGuard::new()
}

/// A spinlock that disables interrupts while it is held.
pub struct IrqSpinLock<T> {
    locked: AtomicBool,
    data: UnsafeCell<T>,
}

// SAFETY: the lock flag serialises CPUs, and masking interrupts makes it non-reentrant on the
// holding CPU: acquiring the same guard twice on one CPU deadlocks by contract, so every
// interrupt-reachable user must never take it recursively.
unsafe impl<T: Send> Sync for IrqSpinLock<T> {}

impl<T> IrqSpinLock<T> {
    /// Create an unlocked lock.
    pub const fn new(value: T) -> Self {
        Self {
            locked: AtomicBool::new(false),
            data: UnsafeCell::new(value),
        }
    }

    /// Acquire the lock, disabling interrupts until the guard drops.
    pub fn lock(&self) -> IrqGuard<'_, T> {
        let flags = disable_interrupts();
        while self.locked.swap(true, Ordering::Acquire) {
            core::hint::spin_loop();
        }
        percpu::preempt_inc();
        IrqGuard { lock: self, flags }
    }
}

/// Guard returned by [`IrqSpinLock::lock`].
pub struct IrqGuard<'a, T> {
    lock: &'a IrqSpinLock<T>,
    flags: u64,
}

impl<T> Deref for IrqGuard<'_, T> {
    type Target = T;

    fn deref(&self) -> &T {
        // SAFETY: the guard proves exclusive access for its lifetime.
        unsafe { &*self.lock.data.get() }
    }
}

impl<T> DerefMut for IrqGuard<'_, T> {
    fn deref_mut(&mut self) -> &mut T {
        // SAFETY: the guard proves exclusive access for its lifetime.
        unsafe { &mut *self.lock.data.get() }
    }
}

impl<T> Drop for IrqGuard<'_, T> {
    fn drop(&mut self) {
        percpu::preempt_dec();
        self.lock.locked.store(false, Ordering::Release);
        restore_interrupts(self.flags);
    }
}

// ---------------------------------------------------------------------------
// Blocking mutex with priority inheritance (L3)
// ---------------------------------------------------------------------------

/// The pure turnstile model, compiled from the same source the host tests run (L1/L2a/L5a/L6a).
/// `kernel/Cargo.toml` is not in this ticket's Owns, so it is included by `#[path]` (S26/S27).
#[path = "../../crates/raven-turnstile/src/turnstile.rs"]
#[allow(dead_code)] // The kernel uses `acquire`/`block`/`wake_one`; the rest of the surface awaits consumers.
#[allow(unexpected_cfgs)] // The `lockstat` feature cfg belongs to the turnstile crate, not the kernel.
#[allow(clippy::indexing_slicing, clippy::arithmetic_side_effects)]
// Pure, host-tested heap/queue math with bounded indices.
mod turnstile;

use turnstile::{Error as TurnstileError, ObjectId, Priority, ThreadId, Turnstiles};

/// The global turnstile registry plus the `tid -> Arc<Thread>` map used to wake handed-off owners.
struct KernelTurnstile {
    turnstiles: Turnstiles,
    threads: BTreeMap<ThreadId, Arc<Thread>>,
}

impl KernelTurnstile {
    fn new() -> Self {
        Self {
            turnstiles: Turnstiles::new(),
            threads: BTreeMap::new(),
        }
    }

    /// Register a thread's base priority once, remembering its `Arc<Thread>` for wakeups.
    fn register(&mut self, tid: ThreadId, thread: &Arc<Thread>) {
        if self.turnstiles.base_priority(tid).is_none() {
            self.turnstiles
                .register(tid, Priority::new(thread.priority))
                .ok();
            self.threads.insert(tid, Arc::clone(thread));
        }
    }
}

static TURNSTILES: IrqSpinLock<Option<KernelTurnstile>> = IrqSpinLock::new(None);
static NEXT_MUTEX_ID: AtomicU32 = AtomicU32::new(1);

/// A blocking mutex with priority inheritance.
///
/// The owner and waiters live in the global turnstile registry keyed by this mutex's object id;
/// contention blocks the caller (via the scheduler's `begin_block`/`block_current`) and unlock
/// hands the lock directly to the highest-priority waiter. A thread blocked on a held mutex
/// raises the owner's *effective* priority (see [`turnstile_effective_priority`]); applying that
/// boost to the scheduler's pick path needs a scheduler hook (see the L3 report).
///
/// Unlike [`IrqSpinLock`], the critical section may block and allocate: it never disables
/// interrupts while held.
#[allow(dead_code)] // L3 primitive; consumers (L5 lockstat, L6 IPC donation) arrive later.
pub struct Mutex<T> {
    object: AtomicU32,
    data: UnsafeCell<T>,
}

// SAFETY: `data` is only accessed by the owning thread through `MutexGuard`; `object` is atomic.
unsafe impl<T: Send> Send for Mutex<T> {}
// SAFETY: same ownership argument as `Send`.
unsafe impl<T: Send> Sync for Mutex<T> {}

#[allow(dead_code)] // L3 primitive; consumers (L5 lockstat, L6 IPC donation) arrive later.
impl<T> Mutex<T> {
    /// Create an unlocked mutex.
    pub const fn new(value: T) -> Self {
        Self {
            object: AtomicU32::new(0),
            data: UnsafeCell::new(value),
        }
    }

    /// The turnstile object id for this mutex, assigned once on first use.
    fn object_id(&self) -> ObjectId {
        let id = self.object.load(Ordering::Acquire);
        if id != 0 {
            return id;
        }
        let candidate = NEXT_MUTEX_ID
            .fetch_add(1, Ordering::Relaxed)
            .wrapping_add(1);
        match self
            .object
            .compare_exchange(0, candidate, Ordering::AcqRel, Ordering::Acquire)
        {
            Ok(_) => candidate,
            Err(existing) => existing,
        }
    }

    /// Acquire the mutex, blocking on contention until ownership is handed off.
    pub fn lock(&self) -> MutexGuard<'_, T> {
        let thread = crate::sched::current_thread().expect("mutex lock: no current thread");
        let tid = thread.tid;
        let object = self.object_id();
        let token = crate::sched::begin_block().expect("mutex lock: scheduler not initialized");
        let acquired = {
            let mut guard = TURNSTILES.lock();
            let kernel = guard.get_or_insert_with(KernelTurnstile::new);
            kernel.register(tid, &thread);
            match kernel.turnstiles.acquire(object, tid) {
                Ok(()) => true,
                Err(TurnstileError::AlreadyOwned { .. }) => {
                    // Block on the owner; the returned `InheritEffect` is the owner boost. The
                    // boost is computed by the turnstile and observable via
                    // `turnstile_effective_priority`; applying it to the scheduler needs a hook.
                    let _ = kernel.turnstiles.block(object, tid);
                    false
                }
                Err(_) => false,
            }
        };
        if acquired {
            crate::sched::end_block(token);
            return MutexGuard { mutex: self };
        }
        crate::sched::block_current();
        crate::sched::end_block(token);
        // We were handed ownership by `unlock` (or a racing wake); return the guard.
        MutexGuard { mutex: self }
    }

    /// Release the mutex, handing it to the highest-priority waiter (or unlocking it).
    fn unlock(&self) {
        let object = self.object_id();
        let next = {
            let mut guard = TURNSTILES.lock();
            let kernel = guard.get_or_insert_with(KernelTurnstile::new);
            kernel
                .turnstiles
                .wake_one(object)
                .and_then(|next_tid| kernel.threads.get(&next_tid).cloned())
        };
        if let Some(next_thread) = next {
            crate::sched::wake(next_thread, None, 0);
        }
    }
}

/// The guard returned by [`Mutex::lock`].
#[allow(dead_code)] // L3 primitive; consumers (L5 lockstat, L6 IPC donation) arrive later.
pub struct MutexGuard<'a, T> {
    mutex: &'a Mutex<T>,
}

#[allow(dead_code)] // L3 primitive; consumers (L5 lockstat, L6 IPC donation) arrive later.
impl<T> Deref for MutexGuard<'_, T> {
    type Target = T;

    fn deref(&self) -> &T {
        // SAFETY: the guard proves the caller is the sole owner of the mutex, so this is the only
        // live reference to `data`.
        unsafe { &*self.mutex.data.get() }
    }
}

#[allow(dead_code)] // L3 primitive; consumers (L5 lockstat, L6 IPC donation) arrive later.
impl<T> DerefMut for MutexGuard<'_, T> {
    fn deref_mut(&mut self) -> &mut T {
        // SAFETY: the guard proves the caller is the sole owner of the mutex, so this is the only
        // live mutable reference to `data`.
        unsafe { &mut *self.mutex.data.get() }
    }
}

#[allow(dead_code)] // L3 primitive; consumers (L5 lockstat, L6 IPC donation) arrive later.
impl<T> Drop for MutexGuard<'_, T> {
    fn drop(&mut self) {
        self.mutex.unlock();
    }
}

/// The current turnstile effective priority of `tid` (the inheritance computation, hosted here).
///
/// This is the priority the scheduler would run `tid` at if the boost were applied; it is exposed
/// for the locking self-test and the lockstat probes. The scheduler's pick path reads
/// `Thread::effective_priority`, which for kernel threads is the non-atomic `Thread::priority`, so
/// applying this value there needs a scheduler hook (see the L3 report).
#[allow(dead_code)] // Read by the locking self-test once its selftest entry is wired.
pub fn turnstile_effective_priority(tid: ThreadId) -> Option<u8> {
    let guard = TURNSTILES.lock();
    guard
        .as_ref()
        .and_then(|kernel| kernel.turnstiles.effective_priority(tid))
        .map(|priority| priority.raw())
}
