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
use alloc::sync::{Arc, Weak};
use core::cell::UnsafeCell;
use core::ops::{Deref, DerefMut};
use core::sync::atomic::{AtomicBool, AtomicU8, AtomicU32, Ordering};

use crate::sched::smp::percpu;
use crate::sched::thread::{NO_BOOST, Thread, ThreadState};

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
#[allow(clippy::indexing_slicing, clippy::arithmetic_side_effects)]
// Pure, host-tested heap/queue math with bounded indices.
mod turnstile;

/// IPC call-edge model (L6a) compiled from the same source as the host tests.
///
/// `donation.rs` resolves its imports through its parent module (`super`), so the turnstile types
/// it names are imported here first; the `#[path]` include keeps one source of truth for the
/// kernel and the host suite.
#[path = "../../crates/raven-turnstile/src/donation.rs"]
#[allow(dead_code)] // The kernel uses `begin_call`/`end_call`; the rest awaits consumers.
#[allow(clippy::indexing_slicing, clippy::arithmetic_side_effects)]
// Pure, host-tested graph math with bounded indices.
mod donation;

#[cfg(feature = "lockstat")]
use alloc::vec::Vec;

use donation::Donation;
#[cfg(feature = "lockstat")]
use turnstile::LockStats;
use turnstile::{CancelOutcome, Error, InheritEffect, ObjectId, Priority, ThreadId, Turnstiles};

/// The global donation graph plus the `tid -> Weak<Thread>` map used to apply and restore boosts.
///
/// The map holds weak references: a thread that exits must not be kept alive (with its kernel
/// stack) just because it once used a lock or an IPC call.
struct KernelTurnstile {
    graph: Donation,
    threads: BTreeMap<ThreadId, Weak<Thread>>,
}

impl KernelTurnstile {
    fn new() -> Self {
        Self {
            graph: Donation::new(),
            threads: BTreeMap::new(),
        }
    }

    /// Register a thread's base priority once and remember it weakly for boost application.
    fn register(&mut self, tid: ThreadId, thread: &Arc<Thread>) {
        if self.graph.base_priority(tid).is_none() {
            self.graph
                .register(tid, Priority::new(thread.priority.load(Ordering::Acquire)))
                .ok();
        }
        self.threads.insert(tid, Arc::downgrade(thread));
    }

    /// Unregister a thread that is leaving.
    ///
    /// FreeBSD `thread_fini`/`turnstile_free` and NetBSD `lwp_dtor`/`lwp_free` release the
    /// thread's turnstile with the thread itself. Raven's graph entry is the equivalent
    /// allocation: dropping the weak map entry and the graph node here does not keep the thread
    /// alive and does not hand a stale base priority to a reused tid.
    fn unregister(&mut self, tid: ThreadId) {
        self.graph.unregister(tid);
        self.threads.remove(&tid);
    }
}

static TURNSTILES: IrqSpinLock<Option<KernelTurnstile>> = IrqSpinLock::new(None);
static NEXT_MUTEX_ID: AtomicU32 = AtomicU32::new(1);

/// Apply `effective` to `thread` as the scheduler boost, clearing it when no donor is active.
fn apply_boost(thread: &Thread, effective: u8) {
    let base = thread.priority.load(Ordering::Acquire);
    if effective < base {
        thread.set_boost(effective);
    } else {
        thread.clear_boost();
    }
}

/// Apply `tid`'s graph effective priority; the caller holds the graph lock.
///
/// Keeping the write inside the same critical section as the graph mutation makes `Thread.boost`
/// and the graph state coherent for any observer that reads the graph under the lock (the
/// self-tests poll it), so a woken owner cannot observe a boost that is still about to be applied.
fn apply_tid_boost(kernel: &KernelTurnstile, tid: ThreadId) {
    let Some(effective) = kernel.graph.effective_priority(tid) else {
        return;
    };
    let Some(thread) = kernel.threads.get(&tid).and_then(Weak::upgrade) else {
        return;
    };
    apply_boost(&thread, effective.raw());
}

/// The registered `Arc<Thread>` for `tid` (weak map; `None` once the thread is gone).
fn registered_thread(tid: ThreadId) -> Option<Arc<Thread>> {
    let guard = TURNSTILES.lock();
    guard
        .as_ref()
        .and_then(|kernel| kernel.threads.get(&tid))
        .and_then(Weak::upgrade)
}

/// Re-apply every registered thread's effective priority; the caller holds the graph lock.
///
/// A call edge can lift a server and, transitively, the owner of a lock the server waits on; a
/// single `set_boost` for the named thread is not enough once the two edge kinds compose. The
/// registered set is small (threads that used a lock or an IPC call) and the weak map drops dead
/// threads, so a full pass is bounded. Only atomics are touched while the graph lock is held.
fn sync_all_boosts_locked(kernel: &KernelTurnstile) {
    for (tid, weak) in &kernel.threads {
        let Some(effective) = kernel.graph.effective_priority(*tid) else {
            continue;
        };
        let Some(thread) = weak.upgrade() else {
            continue;
        };
        apply_boost(&thread, effective.raw());
    }
}

/// Register `thread` with the donation graph so an IPC call edge can name it as the client.
///
/// Called once per request by the client thread; the base priority is captured at that point.
pub fn register_thread(thread: &Arc<Thread>) {
    let mut guard = TURNSTILES.lock();
    let kernel = guard.get_or_insert_with(KernelTurnstile::new);
    kernel.register(thread.tid, thread);
}

/// Unregister a thread at exit: drop its graph node and weak map entry, then reapply the boosts
/// of the remaining threads.
///
/// Called from `sched::exit_thread`. The weak map never kept the dead thread alive, but the graph
/// entry was permanent; at four entries per SSH session it exhausted the 3072-byte class. The
/// graph removal is allocation-free (thread-exit context) and restores any server or lock owner
/// the thread had boosted.
pub fn unregister_thread(tid: ThreadId) {
    let mut guard = TURNSTILES.lock();
    let Some(kernel) = guard.as_mut() else {
        return;
    };
    kernel.unregister(tid);
    sync_all_boosts_locked(kernel);
}

/// K449 diagnostic: `(graph_threads, weak_threads)` for the donation registry.
///
/// `graph_threads` is the base/effective/call graph size; `weak_threads` is the boost map. Both
/// must return to their baseline after a session's threads exit.
#[must_use]
pub fn turnstile_counts() -> (usize, usize) {
    let guard = TURNSTILES.lock();
    guard.as_ref().map_or((0, 0), |kernel| {
        (kernel.graph.registered_threads(), kernel.threads.len())
    })
}

/// Open the IPC call edge `client -> server` and apply the resulting boosts.
///
/// The client must already be registered (see [`register_thread`]); the server is registered
/// here. Returns `false` when the client is unknown, in which case no edge is created.
pub fn donation_begin_call(client: ThreadId, server: &Arc<Thread>) -> bool {
    let mut guard = TURNSTILES.lock();
    let kernel = guard.get_or_insert_with(KernelTurnstile::new);
    kernel.register(server.tid, server);
    if kernel.graph.begin_call(client, server.tid).is_err() {
        return false;
    }
    sync_all_boosts_locked(kernel);
    true
}

/// Close the IPC call edge `client -> server` and restore every boost the edge contributed to.
pub fn donation_end_call(client: ThreadId, server: ThreadId) {
    let mut guard = TURNSTILES.lock();
    if let Some(kernel) = guard.as_mut() {
        // Allocation-free: this runs from process-death/fault/cancel completion paths.
        let _ = kernel.graph.end_call_no_alloc(client, server);
        sync_all_boosts_locked(kernel);
    }
}

/// Wake `thread`, retrying while the scheduler still sees it `Ready`.
///
/// `sched::wake` returns `false` for a `Ready` thread without setting the pending-wake flag: if
/// the target was preempted between `begin_block` and `block_current`, a single wake is lost.
/// The target sets its "asleep" flag after `begin_block`, so retrying until the wake is accepted
/// (or the thread is dead) closes that window without sleeping.
fn wake_blocked(thread: &Arc<Thread>) {
    while !crate::sched::wake(Arc::clone(thread), None, 0) {
        if thread.state() == ThreadState::Dead {
            break;
        }
        core::hint::spin_loop();
    }
}

/// A blocking mutex with priority inheritance.
///
/// The owner and waiters live in the global turnstile registry keyed by this mutex's object id;
/// contention blocks the caller (via the scheduler's `begin_block`/`block_current`) and unlock
/// hands the lock directly to the highest-priority waiter. A thread blocked on a held mutex
/// raises the owner's effective priority, applied to the scheduler through [`Thread::set_boost`]/
/// [`Thread::clear_boost`], and the boost is restored to base when the last waiter leaves.
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
            match kernel.graph.acquire(object, tid) {
                Ok(()) => true,
                Err(Error::AlreadyOwned { .. }) => {
                    // Block on the owner; the returned `InheritEffect` names the owner whose
                    // effective priority the block may have raised. Apply it before the lock is
                    // released so a woken owner never sees a stale boost.
                    if let Ok(effect) = kernel.graph.block(object, tid) {
                        apply_tid_boost(kernel, effect.owner);
                    }
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
        let old_owner = crate::sched::current_thread().map(|thread| thread.tid);
        let next = {
            let mut guard = TURNSTILES.lock();
            let kernel = guard.get_or_insert_with(KernelTurnstile::new);
            let next = kernel.graph.wake_one(object);
            // The releasing owner drops its boost; the handed-off owner inherits any remaining
            // waiter. Both writes land inside the graph's critical section.
            if let Some(old) = old_owner {
                apply_tid_boost(kernel, old);
            }
            if let Some(next_tid) = next {
                apply_tid_boost(kernel, next_tid);
            }
            next
        };
        if let Some(next_tid) = next
            && let Some(next_thread) = registered_thread(next_tid)
        {
            wake_blocked(&next_thread);
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
/// This is the priority the scheduler runs `tid` at once the lock operations have applied it to
/// `Thread.boost`; it is exposed for the self-tests and the lockstat probes.
pub fn turnstile_effective_priority(tid: ThreadId) -> Option<u8> {
    let guard = TURNSTILES.lock();
    guard
        .as_ref()
        .and_then(|kernel| kernel.graph.effective_priority(tid))
        .map(|priority| priority.raw())
}

/// Per-object lock statistics, sorted by object id (empty before the graph exists).
///
/// The counter set is the pure model's `LockStats`: `wait_entries`, `immediate_acquisitions`,
/// `wakes`, `cancels`, `timeouts`, `max_queue_depth`, `max_inheritance_depth` and
/// `inherited_priority_changes` (the "boosts" count: how often an owner's effective priority
/// moved). `timeouts` stays 0 because the pure model has no timeout path.
#[cfg(feature = "lockstat")]
pub fn lockstat() -> Vec<(ObjectId, LockStats)> {
    let guard = TURNSTILES.lock();
    let Some(kernel) = guard.as_ref() else {
        return Vec::new();
    };
    kernel
        .graph
        .stats_objects()
        .map(|object| (object, kernel.graph.stats(object)))
        .collect()
}

/// The `selftest=locking` kernel-thread test: a low-priority owner is boosted by a high-priority
/// waiter, `wake_one` hands the mutex off, and the owner's base priority is restored on unlock.
#[allow(dead_code)] // The dispatcher is x86-64 only (aarch64 has no `selftest=locking` entry yet).
pub fn selftest() {
    static M: Mutex<u32> = Mutex::new(0);
    static OWNER_HELD: AtomicBool = AtomicBool::new(false);
    static OWNER_BOOST: AtomicU8 = AtomicU8::new(NO_BOOST);
    static OWNER_RESTORED: AtomicBool = AtomicBool::new(false);
    static WAITER_GOT_LOCK: AtomicBool = AtomicBool::new(false);

    fn owner() {
        let guard = M.lock();
        OWNER_HELD.store(true, Ordering::Release);
        // Wait for the high-priority waiter to block, which boosts us to its priority.
        loop {
            let boost = crate::sched::current_thread()
                .map(|thread| thread.boost.load(Ordering::Acquire))
                .unwrap_or(NO_BOOST);
            if boost != NO_BOOST {
                OWNER_BOOST.store(boost, Ordering::Release);
                break;
            }
            core::hint::spin_loop();
        }
        drop(guard); // unlock: direct handoff to the waiter
        let restored = crate::sched::current_thread()
            .map(|thread| thread.boost.load(Ordering::Acquire) == NO_BOOST)
            .unwrap_or(false);
        OWNER_RESTORED.store(restored, Ordering::Release);
    }

    fn waiter() {
        while !OWNER_HELD.load(Ordering::Acquire) {
            core::hint::spin_loop();
        }
        let guard = M.lock(); // blocks; the owner's unlock hands the mutex to us
        WAITER_GOT_LOCK.store(true, Ordering::Release);
        drop(guard);
    }

    let owner_thread =
        crate::sched::spawn_kernel("locking-owner", owner).expect("locking: spawn owner");
    owner_thread.priority.store(20, Ordering::Release); // low priority
    let waiter_thread =
        crate::sched::spawn_kernel("locking-waiter", waiter).expect("locking: spawn waiter");
    waiter_thread.priority.store(0, Ordering::Release); // high priority

    // The preemptive tick switches between the spinning threads; wait for both to finish.
    while !OWNER_RESTORED.load(Ordering::Acquire) || !WAITER_GOT_LOCK.load(Ordering::Acquire) {
        core::hint::spin_loop();
    }

    let boost = OWNER_BOOST.load(Ordering::Acquire);
    if boost == 0
        && OWNER_RESTORED.load(Ordering::Acquire)
        && WAITER_GOT_LOCK.load(Ordering::Acquire)
    {
        crate::kprintln!("locking: ok owner_boost=0 handoff=ok restored=ok");
    } else {
        crate::kprintln!(
            "locking: fail boost={boost} restored={} handoff={}",
            OWNER_RESTORED.load(Ordering::Acquire),
            WAITER_GOT_LOCK.load(Ordering::Acquire)
        );
        crate::boot::exit_qemu(0x11);
    }
}

/// The `selftest=lockstat` kernel-thread test: two waiters block on one mutex, the owner is
/// boosted once, the lock is handed off twice, and the counter dump plus the restoration is
/// printed as one machine-readable line.
#[cfg(feature = "lockstat")]
#[allow(dead_code)] // The dispatcher and the arm are cfg'd (x86-64 dispatcher; `lockstat` feature).
pub fn selftest_lockstat() {
    static M: Mutex<u32> = Mutex::new(0);
    static OWNER_HELD: AtomicBool = AtomicBool::new(false);
    static OWNER_ASLEEP: AtomicBool = AtomicBool::new(false);
    static OWNER_BOOST: AtomicU8 = AtomicU8::new(NO_BOOST);
    static OWNER_RESTORED: AtomicBool = AtomicBool::new(false);
    static WAITER_A_GOT: AtomicBool = AtomicBool::new(false);
    static WAITER_B_GOT: AtomicBool = AtomicBool::new(false);

    fn owner() {
        let me = crate::sched::current_thread().expect("lockstat: owner thread");
        me.priority.store(20, Ordering::Release); // low priority: the donee
        let guard = M.lock();
        OWNER_HELD.store(true, Ordering::Release);
        // Sleep while both waiters queue up; the orchestrator wakes us with the retrying wake
        // once the queue depth reaches two, so no spin gate can starve the owner's CPU.
        let token = crate::sched::begin_block().expect("lockstat: owner block");
        OWNER_ASLEEP.store(true, Ordering::Release);
        let _ = crate::sched::block_current();
        crate::sched::end_block(token);
        OWNER_BOOST.store(me.boost.load(Ordering::Acquire), Ordering::Release);
        drop(guard); // handoff to the first waiter
        OWNER_RESTORED.store(
            me.boost.load(Ordering::Acquire) == NO_BOOST,
            Ordering::Release,
        );
    }

    fn waiter_a() {
        let me = crate::sched::current_thread().expect("lockstat: waiter A");
        me.priority.store(0, Ordering::Release); // most urgent: the donor
        while !OWNER_HELD.load(Ordering::Acquire) {
            core::hint::spin_loop();
        }
        let guard = M.lock();
        WAITER_A_GOT.store(true, Ordering::Release);
        drop(guard); // handoff to waiter B
    }

    fn waiter_b() {
        let me = crate::sched::current_thread().expect("lockstat: waiter B");
        me.priority.store(1, Ordering::Release);
        while !OWNER_HELD.load(Ordering::Acquire) {
            core::hint::spin_loop();
        }
        let guard = M.lock();
        WAITER_B_GOT.store(true, Ordering::Release);
        drop(guard);
    }

    // Keep the orchestrator below every worker so a ready worker always wins the pick.
    let main = crate::sched::current_thread();
    let saved_priority = main
        .as_ref()
        .map(|thread| thread.priority.load(Ordering::Acquire));
    if let Some(thread) = main.as_ref() {
        thread.priority.store(31, Ordering::Release);
    }
    let owner_thread =
        crate::sched::spawn_kernel("lockstat-owner", owner).expect("lockstat: spawn owner");
    owner_thread.priority.store(20, Ordering::Release);
    let waiter_a_thread =
        crate::sched::spawn_kernel("lockstat-waiter-a", waiter_a).expect("lockstat: spawn A");
    waiter_a_thread.priority.store(0, Ordering::Release);
    let waiter_b_thread =
        crate::sched::spawn_kernel("lockstat-waiter-b", waiter_b).expect("lockstat: spawn B");
    waiter_b_thread.priority.store(1, Ordering::Release);

    let object = M.object_id();
    let mut owner_woken = false;
    while !OWNER_RESTORED.load(Ordering::Acquire)
        || !WAITER_A_GOT.load(Ordering::Acquire)
        || !WAITER_B_GOT.load(Ordering::Acquire)
    {
        // Wake the sleeping owner only once both waiters are queued, so the queue depth reaches
        // two and the boost is visible; the retrying wake survives a preemption window.
        if !owner_woken && OWNER_ASLEEP.load(Ordering::Acquire) {
            let queued = lockstat()
                .into_iter()
                .find(|(id, _)| *id == object)
                .is_some_and(|(_, stats)| stats.wait_entries >= 2);
            if queued {
                owner_woken = true;
                wake_blocked(&owner_thread);
            }
        }
        core::hint::spin_loop();
    }
    if let (Some(thread), Some(priority)) = (main.as_ref(), saved_priority) {
        thread.priority.store(priority, Ordering::Release);
    }

    let stats = lockstat()
        .into_iter()
        .find(|(id, _)| *id == object)
        .map(|(_, stats)| stats);
    let owner_boost = OWNER_BOOST.load(Ordering::Acquire);
    let Some(stats) = stats else {
        crate::kprintln!("lockstat: fail object={object} stats=missing");
        crate::boot::exit_qemu(0x13);
    };
    // The waiters race for the turnstile; the first one lowers the owner's priority and the
    // second one (priority 1) does not lower it further when A (priority 0) arrived first.
    if stats.wait_entries == 2
        && stats.wakes == 2
        && stats.cancels == 0
        && stats.timeouts == 0
        && stats.max_queue_depth == 2
        && (1..=2).contains(&stats.inherited_priority_changes)
        && owner_boost == 0
        && OWNER_RESTORED.load(Ordering::Acquire)
        && WAITER_A_GOT.load(Ordering::Acquire)
        && WAITER_B_GOT.load(Ordering::Acquire)
    {
        crate::kprintln!(
            "lockstat: ok object={object} wait_entries={} wakes={} cancels={} timeouts={} boosts={} max_queue={} owner_boost={owner_boost} restored=ok",
            stats.wait_entries,
            stats.wakes,
            stats.cancels,
            stats.timeouts,
            stats.inherited_priority_changes,
            stats.max_queue_depth
        );
    } else {
        crate::kprintln!(
            "lockstat: fail object={object} wait_entries={} wakes={} cancels={} timeouts={} boosts={} max_queue={} owner_boost={owner_boost} restored={} handoff={}",
            stats.wait_entries,
            stats.wakes,
            stats.cancels,
            stats.timeouts,
            stats.inherited_priority_changes,
            stats.max_queue_depth,
            OWNER_RESTORED.load(Ordering::Acquire),
            WAITER_A_GOT.load(Ordering::Acquire) && WAITER_B_GOT.load(Ordering::Acquire)
        );
        crate::boot::exit_qemu(0x13);
    }
}

/// The `selftest=ipc-donation` kernel-thread test: a high-priority client opens a call edge to a
/// low-priority server, the server waits on a mutex held by a lower-priority owner, and the
/// client's priority reaches the owner through the call edge plus the turnstile wait edge. Every
/// boost is restored when the lock is released and the call completes.
#[allow(dead_code)] // The dispatcher is x86-64 only (aarch64 has no `selftest=ipc-donation` entry yet).
pub fn selftest_ipc_donation() {
    static M: Mutex<u32> = Mutex::new(0);
    static CLIENT_THREAD: IrqSpinLock<Option<Arc<Thread>>> = IrqSpinLock::new(None);
    static CLIENT_TID: AtomicU32 = AtomicU32::new(0);
    static CLIENT_ASLEEP: AtomicBool = AtomicBool::new(false);
    static OWNER_HELD: AtomicBool = AtomicBool::new(false);
    static OWNER_ASLEEP: AtomicBool = AtomicBool::new(false);
    static CLIENT_RESTORED: AtomicBool = AtomicBool::new(false);
    static OWNER_BOOST: AtomicU8 = AtomicU8::new(NO_BOOST);
    static OWNER_RESTORED: AtomicBool = AtomicBool::new(false);
    static SERVER_BOOST: AtomicU8 = AtomicU8::new(NO_BOOST);
    static SERVER_RESTORED: AtomicBool = AtomicBool::new(false);

    fn client() {
        let me = crate::sched::current_thread().expect("ipc-donation: client thread");
        me.priority.store(0, Ordering::Release); // highest: the donor
        register_thread(&me);
        CLIENT_TID.store(me.tid, Ordering::Release);
        CLIENT_THREAD.lock().replace(Arc::clone(&me));
        // The client blocks on the reply; the server owns the call edge from here on. The asleep
        // flag is set after `begin_block`, so the waker's retry cannot land before the flag.
        let token = crate::sched::begin_block().expect("ipc-donation: client block");
        CLIENT_ASLEEP.store(true, Ordering::Release);
        let _ = crate::sched::block_current();
        crate::sched::end_block(token);
        CLIENT_RESTORED.store(
            me.boost.load(Ordering::Acquire) == NO_BOOST,
            Ordering::Release,
        );
    }

    fn owner() {
        let me = crate::sched::current_thread().expect("ipc-donation: owner thread");
        me.priority.store(5, Ordering::Release); // above the server, below the client
        let guard = M.lock();
        OWNER_HELD.store(true, Ordering::Release);
        // Sleep while holding the mutex: the server's lock wait must do the boosting and the
        // orchestrator wakes us with the retrying wake once the wait edge exists.
        let token = crate::sched::begin_block().expect("ipc-donation: owner block");
        OWNER_ASLEEP.store(true, Ordering::Release);
        let _ = crate::sched::block_current();
        crate::sched::end_block(token);
        OWNER_BOOST.store(me.boost.load(Ordering::Acquire), Ordering::Release);
        drop(guard); // unlock hands the mutex to the server
        OWNER_RESTORED.store(
            me.boost.load(Ordering::Acquire) == NO_BOOST,
            Ordering::Release,
        );
    }

    fn server() {
        let me = crate::sched::current_thread().expect("ipc-donation: server thread");
        me.priority.store(10, Ordering::Release);
        while !CLIENT_ASLEEP.load(Ordering::Acquire) || !OWNER_HELD.load(Ordering::Acquire) {
            core::hint::spin_loop();
        }
        let client_tid = CLIENT_TID.load(Ordering::Acquire);
        if !donation_begin_call(client_tid, &me) {
            SERVER_RESTORED.store(false, Ordering::Release);
            return;
        }
        SERVER_BOOST.store(me.boost.load(Ordering::Acquire), Ordering::Release);
        // Block on the mutex the sleeping owner holds; the block applies the inherited priority
        // to the owner. The orchestrator wakes the owner once this wait edge exists.
        let guard = M.lock();
        drop(guard);
        donation_end_call(client_tid, me.tid);
        SERVER_RESTORED.store(
            me.boost.load(Ordering::Acquire) == NO_BOOST,
            Ordering::Release,
        );
        if let Some(client) = CLIENT_THREAD.lock().clone() {
            wake_blocked(&client);
        }
    }

    // Keep the orchestrator below every worker so a ready worker always wins the pick.
    let main = crate::sched::current_thread();
    let saved_priority = main
        .as_ref()
        .map(|thread| thread.priority.load(Ordering::Acquire));
    if let Some(thread) = main.as_ref() {
        thread.priority.store(31, Ordering::Release);
    }
    let client_thread = crate::sched::spawn_kernel("ipc-donation-client", client)
        .expect("ipc-donation: spawn client");
    client_thread.priority.store(0, Ordering::Release);
    let owner_thread =
        crate::sched::spawn_kernel("ipc-donation-owner", owner).expect("ipc-donation: spawn owner");
    owner_thread.priority.store(5, Ordering::Release);
    let server_thread = crate::sched::spawn_kernel("ipc-donation-server", server)
        .expect("ipc-donation: spawn server");
    server_thread.priority.store(10, Ordering::Release);

    let mut owner_woken = false;
    while !CLIENT_ASLEEP.load(Ordering::Acquire)
        || !OWNER_HELD.load(Ordering::Acquire)
        || !SERVER_RESTORED.load(Ordering::Acquire)
        || !OWNER_RESTORED.load(Ordering::Acquire)
        || !CLIENT_RESTORED.load(Ordering::Acquire)
    {
        // The owner sleeps holding the mutex; wake it only once the server's lock wait has
        // applied the inherited priority, so it observes the boost rather than racing the wake.
        if !owner_woken
            && OWNER_ASLEEP.load(Ordering::Acquire)
            && turnstile_effective_priority(owner_thread.tid) == Some(0)
        {
            owner_woken = true;
            wake_blocked(&owner_thread);
        }
        core::hint::spin_loop();
    }

    if let (Some(thread), Some(priority)) = (main.as_ref(), saved_priority) {
        thread.priority.store(priority, Ordering::Release);
    }

    let server_boost = SERVER_BOOST.load(Ordering::Acquire);
    let owner_boost = OWNER_BOOST.load(Ordering::Acquire);
    if server_boost == 0
        && owner_boost == 0
        && SERVER_RESTORED.load(Ordering::Acquire)
        && OWNER_RESTORED.load(Ordering::Acquire)
        && CLIENT_RESTORED.load(Ordering::Acquire)
    {
        crate::kprintln!(
            "ipc-donation: ok client_boost=0 server_boost=0 owner_boost=0 restored=ok"
        );
    } else {
        crate::kprintln!(
            "ipc-donation: fail server_boost={server_boost} owner_boost={owner_boost} server_restored={} owner_restored={} client_restored={}",
            SERVER_RESTORED.load(Ordering::Acquire),
            OWNER_RESTORED.load(Ordering::Acquire),
            CLIENT_RESTORED.load(Ordering::Acquire)
        );
        crate::boot::exit_qemu(0x12);
    }
}
