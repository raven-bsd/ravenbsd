// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Kernel event-queue objects and timer/user-event readiness state.

#[path = "../../../crates/raven-evq-core/src/lib.rs"]
pub(crate) mod evq_core;

use core::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use alloc::sync::Arc;
use alloc::vec::Vec;
use raven_abi::errno::Errno;
use spin::Mutex;

use crate::sched::wait::WaitQueue;
use crate::sync::IrqSpinLock;

pub use self::evq_core::SIGNALED;
#[cfg(target_arch = "x86_64")]
pub use self::evq_core::{HUP, READABLE};

/// An event queue registration table with waiters.
#[allow(dead_code)] // Wake integration is completed by the source hooks in this module.
pub struct EventQueue {
    /// Process whose handle values form registration keys.
    pub owner: u64,
    /// The pure edge/level state machine.
    pub state: Mutex<evq_core::Queue>,
    /// Threads blocked in `EVQ_WAIT`.
    pub waiters: WaitQueue,
    /// Only one thread may block in one wait call.
    pub waiting: AtomicBool,
}

impl EventQueue {
    /// Construct an empty event queue.
    pub const fn new(owner: u64) -> Self {
        Self {
            owner,
            state: Mutex::new(evq_core::Queue::new()),
            waiters: WaitQueue::new(),
            waiting: AtomicBool::new(false),
        }
    }

    /// Wake a waiter after a source transition.
    #[allow(dead_code)] // Source hooks call this once they publish readiness.
    pub fn notify(&self) {
        let _ = self.waiters.wake_one(self.owner, 0);
    }
}

/// A monotonic timer object.
pub struct Timer {
    deadline: AtomicU64,
    interval: AtomicU64,
    quota: Arc<core::sync::atomic::AtomicU32>,
}

impl Timer {
    /// Construct a disarmed timer.
    pub fn new(quota: Arc<core::sync::atomic::AtomicU32>) -> Self {
        Self {
            deadline: AtomicU64::new(0),
            interval: AtomicU64::new(0),
            quota,
        }
    }

    /// Arm or disarm the timer. Periodic deadlines advance from their prior deadline.
    pub fn set(&self, now: u64, initial: u64, interval: u64) {
        if initial == 0 {
            self.interval.store(0, Ordering::Release);
            self.deadline.store(0, Ordering::Release);
            return;
        }
        self.interval.store(interval, Ordering::Release);
        self.deadline
            .store(now.saturating_add(initial), Ordering::Release);
    }

    /// Ready state at `now`, without consuming a period.
    ///
    /// This is the predicate form used by readiness scans and enqueue checks; it must stay
    /// side-effect free so probing twice cannot lose a timer event.
    pub fn peek(&self, now: u64) -> bool {
        let deadline = self.deadline.load(Ordering::Acquire);
        deadline != 0 && now >= deadline
    }

    /// Consume the current period, advancing a periodic timer without drift.
    ///
    /// Returns false when the timer is not ready.  The compare-exchange makes a second consumer
    /// of the same period (another event queue watching the same timer) a no-op instead of
    /// skipping a whole period.
    pub fn consume(&self, now: u64) -> bool {
        loop {
            let deadline = self.deadline.load(Ordering::Acquire);
            if deadline == 0 || now < deadline {
                return false;
            }
            let interval = self.interval.load(Ordering::Acquire);
            let next = if interval == 0 {
                0
            } else {
                let elapsed = now.saturating_sub(deadline);
                let periods = elapsed.checked_div(interval).unwrap_or(0).saturating_add(1);
                deadline.saturating_add(periods.saturating_mul(interval))
            };
            if self
                .deadline
                .compare_exchange(deadline, next, Ordering::AcqRel, Ordering::Acquire)
                .is_ok()
            {
                return true;
            }
        }
    }
}

impl Drop for Timer {
    fn drop(&mut self) {
        self.quota.fetch_sub(1, Ordering::AcqRel);
    }
}

/// A pure user-space doorbell.
pub struct UserEvent {
    signalled: AtomicBool,
}

/// Maximum registered event queues.  The interrupt-context notification scan is bounded by this
/// constant; `EVQ_CREATE` fails with `ENOSPC` when it is exhausted.
pub const MAX_QUEUES: usize = 64;

/// Queues registered for broad source notifications.
///
/// The lock is interrupt-safe: `notify_all` runs from the timer interrupt, and a process-context
/// holder (registration or handle close) has interrupts disabled, so the handler on that CPU
/// cannot observe the lock held.
static QUEUES: IrqSpinLock<Vec<alloc::sync::Weak<EventQueue>>> = IrqSpinLock::new(Vec::new());

/// Record a queue so handle closure can synchronously delete its registrations.
pub fn register_queue(queue: &Arc<EventQueue>) -> Result<(), Errno> {
    let mut queues = QUEUES.lock();
    queues.retain(|weak| weak.strong_count() != 0);
    if queues.len() >= MAX_QUEUES {
        return Err(Errno::ENOSPC);
    }
    queues.push(Arc::downgrade(queue));
    Ok(())
}

/// Wake event-queue waiters after a source transition. The ready scan filters queues that do not
/// watch the source, so this broad notification preserves correctness without interrupt-context
/// allocation.
pub fn notify_all() {
    // This also runs from the timer interrupt.  Do not build a temporary Vec or otherwise
    // allocate here; `Weak::upgrade` is allocation-free and each queue has one waiter at most.
    // The common boot case has no queues at all: check that before touching the stack array.
    if QUEUES.lock().is_empty() {
        return;
    }
    // Copy the strong references under the lock, then wake outside it: the wait-queue and
    // scheduler locks are unrelated to the queue table.
    let mut ready: [Option<Arc<EventQueue>>; MAX_QUEUES] = [const { None }; MAX_QUEUES];
    {
        let queues = QUEUES.lock();
        for (slot, weak) in ready.iter_mut().zip(queues.iter()) {
            *slot = weak.upgrade();
        }
    }
    for queue in ready.iter().flatten() {
        queue.notify();
    }
}

/// Event registrations removed by the close hook.
///
/// The S38 evidence print reports the delta between process exits: a close path that forgets the
/// hook shows a short count.  Counting removed registrations (not hook calls) keeps the evidence
/// independent of unrelated handle closes such as a spawn's stack object.
static REMOVED_REGISTRATIONS: core::sync::atomic::AtomicU64 = core::sync::atomic::AtomicU64::new(0);
/// [`REMOVED_REGISTRATIONS`] observed at the previous process exit (the evidence delta base).
#[cfg(target_arch = "x86_64")]
static REMOVED_AT_EXIT: core::sync::atomic::AtomicU64 = core::sync::atomic::AtomicU64::new(0);

/// True when the S38 close-hook evidence print is enabled (`s38-eventcheck` on the command line).
#[cfg(target_arch = "x86_64")]
fn close_evidence() -> bool {
    crate::memory::paging::cmdline()
        .split_ascii_whitespace()
        .any(|token| token == "s38-eventcheck" || token.starts_with("s38-eventcheck="))
}

/// Upgraded queue snapshot.  Allocation-free: `notify_all` runs from the timer interrupt, and the
/// same bound/pattern is reused by the close and exit hooks so they stay safe in any context.
///
/// Kept out of line: the 1 KiB array must live in this frame, never in a syscall-dispatch or
/// teardown frame (the kernel stacks have a narrow margin at the teardown evidence parses).
#[inline(never)]
fn queue_snapshot() -> [Option<Arc<EventQueue>>; MAX_QUEUES] {
    let mut queues: [Option<Arc<EventQueue>>; MAX_QUEUES] = [const { None }; MAX_QUEUES];
    let table = QUEUES.lock();
    for (slot, weak) in queues.iter_mut().zip(table.iter()) {
        *slot = weak.upgrade();
    }
    queues
}

/// Remove `handle` from every event queue owned by `process` and count its registration.
///
/// This is the event part of the close hook.  It is idempotent by construction (`Queue::close` is
/// a retain over the key), so a handle with no registration is a no-op and one successful close
/// can never deregister another close's key.
#[inline(never)]
fn deregister(process: &Arc<crate::process::Process>, handle: raven_abi::types::Handle) {
    let queues = queue_snapshot();
    let mut removed = 0usize;
    for queue in queues.iter().flatten() {
        if queue.owner == process.id {
            let mut state = queue.state.lock();
            let before = state.len();
            state.close(handle.0);
            removed = removed.saturating_add(before.saturating_sub(state.len()));
        }
    }
    REMOVED_REGISTRATIONS.fetch_add(removed as u64, core::sync::atomic::Ordering::Relaxed);
}

/// The post-commit event close hook.
///
/// [`close_handle`] below is the preferred entry point because it can also cancel an IRQ waiter
/// while the slot is still live.  This entry point survives for the user `HANDLE_CLOSE` path,
/// which closes the table first; it removes the handle's registrations and does not touch the
/// (already invalidated) slot.
pub fn on_handle_close(process: &Arc<crate::process::Process>, handle: raven_abi::types::Handle) {
    deregister(process, handle);
}

/// Close `handle` in `process` and run the close hook exactly once.
///
/// This is the single kernel-side close primitive: scheme-file and request-handle close,
/// thread-creation rollback and the process-exit sweep all use it, and the user `HANDLE_CLOSE`
/// path calls [`on_handle_close`] so every close shares the same event hook.  The IRQ object is
/// captured before the close so a blocked `IRQ_WAIT` can be cancelled when the caller has the
/// process context; the event registrations are removed only after the close commits, so a stale
/// handle fails with `EBADF` and leaves every registration intact.
#[inline(never)]
pub fn close_handle(
    process: &Arc<crate::process::Process>,
    handle: raven_abi::types::Handle,
) -> Result<(), Errno> {
    #[cfg(target_arch = "x86_64")]
    let irq = {
        let table = process.handles.lock();
        match table.get(handle, raven_abi::types::Rights::NONE) {
            Ok(crate::process::handle::Object::Irq(irq)) => Some(Arc::clone(irq)),
            _ => None,
        }
    };
    process.handles.lock().close(handle)?;
    #[cfg(target_arch = "x86_64")]
    if let Some(irq) = irq {
        irq.cancel_waiters(handle.0);
    }
    deregister(process, handle);
    Ok(())
}

/// Process-exit close hook: drop every event registration owned by the dying process.
///
/// The teardown calls this before it replaces the handle table.  Every queue with this owner is
/// cleared in one place, so no registration outlives the handle table that keyed it and no
/// waiter is left on a source whose process is gone.  The state lock is released before the
/// waiters are woken to keep the queue lock order (`state` before `waiters`) one-way.
///
/// x86-64 only for now: the scheme IPC layer that calls it (and whose close paths share the
/// hook) is not ported yet.
#[cfg(target_arch = "x86_64")]
#[inline(never)]
pub fn on_process_exit(process: &Arc<crate::process::Process>) {
    let queues = queue_snapshot();
    let mut events = 0usize;
    let mut cleared = 0usize;
    for queue in queues.iter().flatten() {
        if queue.owner != process.id {
            continue;
        }
        let registrations = {
            let mut state = queue.state.lock();
            let count = state.len();
            *state = evq_core::Queue::new();
            count
        };
        events = events.saturating_add(registrations);
        cleared = cleared.saturating_add(1);
        queue.waiting.store(false, Ordering::Release);
        let _ = queue.waiters.wake_all();
    }
    if !close_evidence() {
        return;
    }
    let total = queues
        .iter()
        .flatten()
        .map(|queue| queue.state.lock().len())
        .sum::<usize>();
    let closes = REMOVED_REGISTRATIONS.load(Ordering::Relaxed);
    let delta = closes.saturating_sub(REMOVED_AT_EXIT.swap(closes, Ordering::AcqRel));
    crate::kprintln!(
        "s38: exit pid={} events={} cleared={} total={} removed={}",
        process.id,
        events,
        cleared,
        total,
        delta
    );
}

impl UserEvent {
    /// Construct an unsignalled doorbell.
    pub const fn new() -> Self {
        Self {
            signalled: AtomicBool::new(false),
        }
    }

    /// Set the level and return whether this was a new transition.
    pub fn signal(&self) -> bool {
        !self.signalled.swap(true, Ordering::AcqRel)
    }

    /// Consume the reported doorbell level.
    #[allow(dead_code)] // EVQ_WAIT consumes the level after selecting this source.
    pub fn take(&self) -> bool {
        self.signalled.swap(false, Ordering::AcqRel)
    }

    /// Read the current level.
    pub fn ready(&self) -> bool {
        self.signalled.load(Ordering::Acquire)
    }
}
