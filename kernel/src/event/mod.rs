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

/// Remove a closing handle from every queue in its process before the close returns.
/// Remove `handle` from every registered event queue and cancel a blocked IRQ waiter.
///
/// Called before the handle table drops the object.  For an `Irq`, every waiter is woken and
/// revalidates its own handle: the closing thread runs the close syscall to completion on the
/// non-preemptible CPU, so a waiter that resumes afterwards sees `EBADF` (or its own cancelled
/// handle) and returns `EIO`.  The waiter's `Arc` keeps the object alive until it returns.
pub fn on_handle_close(process: &Arc<crate::process::Process>, handle: raven_abi::types::Handle) {
    #[cfg(target_arch = "x86_64")]
    {
        let irq = {
            let table = process.handles.lock();
            match table.get(handle, raven_abi::types::Rights::NONE) {
                Ok(crate::process::handle::Object::Irq(irq)) => Some(Arc::clone(irq)),
                _ => None,
            }
        };
        if let Some(irq) = irq {
            irq.cancel_waiters(handle.0);
        }
    }
    let mut queues: [Option<Arc<EventQueue>>; MAX_QUEUES] = [const { None }; MAX_QUEUES];
    {
        let table = QUEUES.lock();
        for (slot, weak) in queues.iter_mut().zip(table.iter()) {
            *slot = weak.upgrade();
        }
    }
    for queue in queues.iter().flatten() {
        if queue.owner == process.id {
            queue.state.lock().close(handle.0);
        }
    }
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
