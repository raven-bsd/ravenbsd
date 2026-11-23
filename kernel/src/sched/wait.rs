// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Wait queues built on the scheduler's block/wake primitives.
//!
//! A waiter carries the key it waits for, so one 256-bucket futex table can share queues without
//! mixing addresses in the same bucket. `enqueue_if` evaluates its condition under the queue
//! lock, which is what makes the futex check-and-enqueue sequence atomic against a waker.
//!
//! Lock order: a wait-queue lock is never held while taking the run-queue lock. `wake` removes
//! entries under the bucket lock, releases it, and only then calls [`super::wake`].
//!
//! The futex/syscall path only exists on x86-64 today; aarch64 compiles the module for S14c and
//! does not yet call it.

#![allow(dead_code)]

use alloc::sync::Arc;
use alloc::vec::Vec;
use core::ptr;
use core::sync::atomic::{AtomicBool, AtomicPtr, Ordering};

use raven_abi::errno::Errno;

use super::thread::Thread;
use crate::sync::IrqSpinLock;

/// One blocked thread and the address space/address pair it waits on.
pub struct Waiter {
    /// Address-space id (process id for private futexes).
    pub space: u64,
    /// Futex address.
    pub addr: u64,
    /// The blocked thread.
    pub thread: Arc<Thread>,
}

/// A FIFO wait queue.
pub struct WaitQueue {
    waiters: IrqSpinLock<Vec<Waiter>>,
    /// Global intrusive list of live queues, used to purge a dying thread's entries.
    next: AtomicPtr<WaitQueue>,
    /// True while this queue is on the global list.
    listed: AtomicBool,
}

/// Head of the global queue list and its mutation lock.
static QUEUE_HEAD: AtomicPtr<WaitQueue> = AtomicPtr::new(ptr::null_mut());
static QUEUE_LOCK: IrqSpinLock<()> = IrqSpinLock::new(());

impl WaitQueue {
    /// Create an empty queue.
    pub const fn new() -> Self {
        Self {
            waiters: IrqSpinLock::new(Vec::new()),
            next: AtomicPtr::new(ptr::null_mut()),
            listed: AtomicBool::new(false),
        }
    }

    /// Register this queue for the exit-time purge (idempotent, allocation-free).
    fn register(&self) {
        let _guard = QUEUE_LOCK.lock();
        if self.listed.swap(true, Ordering::AcqRel) {
            return;
        }
        let head = QUEUE_HEAD.load(Ordering::Acquire);
        self.next.store(head, Ordering::Release);
        QUEUE_HEAD.store(
            self as *const WaitQueue as *mut WaitQueue,
            Ordering::Release,
        );
    }

    /// Remove every entry for `thread_id` from every live queue.
    ///
    /// A direct `sched::wake` (signal kick, thread-handle wake) can leave the waiter's entry
    /// behind; the queue owns the `Arc<Thread>`, so the entry would outlive the thread and hold
    /// its process. FreeBSD `kern_sleepq.c:sleepq_remove`/`sleepq_abort` removes the thread from
    /// its sleep queue at exit; this is the equivalent over all Raven wait queues.
    pub fn purge_thread(thread_id: u64) {
        let _guard = QUEUE_LOCK.lock();
        let mut queue = QUEUE_HEAD.load(Ordering::Acquire);
        while !queue.is_null() {
            // SAFETY: the registry lock keeps every listed queue alive until its Drop unlinks
            // it under the same lock.
            let current = unsafe { &*queue };
            current.remove_thread(thread_id);
            queue = current.next.load(Ordering::Acquire);
        }
    }

    /// Wake and remove every waiter whose **thread** belongs to `process_id`.
    ///
    /// Object queues (pipe, duplex, pager, IRQ, event, console, record lock) key their entries by
    /// an object id, not by the process id that `wake_process` matches, so the process sweep must
    /// select on `waiter.thread.process.id`. Removing the entry drops the queue's `Arc<Thread>`;
    /// the `EIO` wake makes the terminal thread run its normal exit path.
    pub fn wake_threads_of_process(&self, process_id: u64) -> usize {
        let mut count = 0usize;
        loop {
            let waiter = {
                let mut waiters = self.waiters.lock();
                waiters
                    .iter()
                    .position(|waiter| waiter.thread.process.id == process_id)
                    .map(|index| waiters.remove(index).thread)
            };
            let Some(thread) = waiter else {
                break;
            };
            if super::wake(thread, None, Errno::EIO.to_ret()) {
                count = count.saturating_add(1);
            }
        }
        count
    }

    /// Wake and remove every waiter of `process_id` from every live queue.
    ///
    /// This is the object-queue half of process teardown: a thread can be blocked on a queue
    /// inside a pipe, duplex or memory object that is no longer in the dying handle table (the
    /// syscall frame is the last owner). The queue registry reaches it; the wait-queue entry is
    /// removed and the thread is woken with `EIO` so it runs the terminal exit path.
    pub fn wake_all_queues_for_process(process_id: u64) -> usize {
        let _guard = QUEUE_LOCK.lock();
        let mut count = 0usize;
        let mut queue = QUEUE_HEAD.load(Ordering::Acquire);
        while !queue.is_null() {
            // SAFETY: the registry lock keeps every listed queue alive until its Drop unlinks
            // it under the same lock.
            let current = unsafe { &*queue };
            count = count.saturating_add(current.wake_threads_of_process(process_id));
            queue = current.next.load(Ordering::Acquire);
        }
        count
    }

    /// Enqueue the current thread for `key` when `condition` returns true.
    ///
    /// The condition runs while the queue lock is held and may read user memory through the
    /// fault-safe usercopy layer. A false result enqueues nothing (`-EAGAIN` for futex).
    pub fn enqueue_if(
        &self,
        space: u64,
        addr: u64,
        condition: impl FnOnce() -> Result<bool, Errno>,
    ) -> Result<bool, Errno> {
        let thread = super::current_thread().ok_or(Errno::ESRCH)?;
        self.register();
        crate::sched::smp::percpu::note_wait_phase(1);
        let mut waiters = self.waiters.lock();
        crate::sched::smp::percpu::note_wait_phase(2);
        let queued = match condition() {
            Ok(queued) => queued,
            Err(error) => {
                drop(waiters);
                crate::sched::smp::percpu::note_wait_phase(0);
                return Err(error);
            }
        };
        if !queued {
            drop(waiters);
            crate::sched::smp::percpu::note_wait_phase(0);
            return Ok(false);
        }
        crate::sched::smp::percpu::note_wait_phase(3);
        waiters.push(Waiter {
            space,
            addr,
            thread,
        });
        drop(waiters);
        crate::sched::smp::percpu::note_wait_phase(0);
        Ok(true)
    }

    /// Wake up to `max` waiters for `(space, addr)`, returning how many actually woke.
    ///
    /// Allocation-free: the teardown and fault paths wake waiters on a bounded kernel stack, so
    /// this removes at most `max` matching entries one at a time and never builds a temporary
    /// `Vec`.  The wait-queue lock is released before the scheduler lock, preserving the usual
    /// wait-queue-to-scheduler order.
    pub fn wake(&self, space: u64, addr: u64, max: usize) -> usize {
        let mut matched = 0usize;
        let mut count = 0usize;
        while matched < max {
            crate::sched::smp::percpu::note_wait_phase(10);
            let waiter = {
                let mut waiters = self.waiters.lock();
                crate::sched::smp::percpu::note_wait_phase(11);
                waiters
                    .iter()
                    .position(|waiter| waiter.space == space && waiter.addr == addr)
                    .map(|index| waiters.remove(index).thread)
            };
            let Some(thread) = waiter else {
                break;
            };
            matched = matched.saturating_add(1);
            crate::sched::smp::percpu::note_wait_phase(12);
            if super::wake(thread, None, 0) {
                count = count.saturating_add(1);
            }
        }
        crate::sched::smp::percpu::note_wait_phase(0);
        count
    }

    /// Wake one waiter for a key without allocating temporary storage.
    ///
    /// Event queues allow exactly one blocking waiter, and timer interrupt delivery must not
    /// allocate.  The waiter is removed before scheduler state is touched, preserving the usual
    /// wait-queue-to-scheduler lock order.
    pub fn wake_one(&self, space: u64, addr: u64) -> usize {
        let waiter = {
            let mut waiters = self.waiters.lock();
            waiters
                .iter()
                .position(|waiter| waiter.space == space && waiter.addr == addr)
                .map(|index| waiters.remove(index).thread)
        };
        if let Some(thread) = waiter {
            usize::from(super::wake(thread, None, 0))
        } else {
            0
        }
    }

    /// Wake every waiter, returning how many actually woke.
    ///
    /// Allocation-free for the same reason as [`Self::wake`]: `PendingRequest::complete` runs
    /// from process teardown, including the page-fault path, and the old temporary
    /// `Vec<Arc<Thread>>` was the deepest allocation that overflowed the kernel stack.
    #[allow(dead_code)] // used by PROC_WAIT in S24
    pub fn wake_all(&self) -> usize {
        let mut count = 0usize;
        loop {
            let waiter = {
                let mut waiters = self.waiters.lock();
                waiters.pop().map(|waiter| waiter.thread)
            };
            let Some(thread) = waiter else {
                break;
            };
            if super::wake(thread, None, 0) {
                count = count.saturating_add(1);
            }
        }
        count
    }

    /// Key of the oldest registered waiter (K391 diagnostic).
    pub fn first_key(&self) -> Option<(u64, u64)> {
        self.waiters
            .lock()
            .first()
            .map(|waiter| (waiter.space, waiter.addr))
    }

    /// Remove a stale waiter after its timeout won the race.
    pub fn remove(&self, space: u64, addr: u64, thread_id: u64) {
        let mut waiters = self.waiters.lock();
        let mut index = 0usize;
        while index < waiters.len() {
            let stale = waiters.get(index).is_some_and(|waiter| {
                waiter.space == space && waiter.addr == addr && waiter.thread.id == thread_id
            });
            if stale {
                let _ = waiters.remove(index);
            } else {
                index = index.saturating_add(1);
            }
        }
    }

    /// Drop every waiter belonging to `thread_id` without waking it.
    ///
    /// Used by thread exit, where the thread is already leaving the scheduler.
    pub fn remove_thread(&self, thread_id: u64) {
        let mut waiters = self.waiters.lock();
        waiters.retain(|waiter| waiter.thread.id != thread_id);
    }

    /// True when `thread_id` still has an entry on this queue (S36 holder diagnostic).
    pub fn contains_thread(&self, thread_id: u64) -> bool {
        self.waiters
            .lock()
            .iter()
            .any(|waiter| waiter.thread.id == thread_id)
    }

    /// Non-blocking [`Self::contains_thread`] for the metrics-thread census.
    pub fn try_contains_thread(&self, thread_id: u64) -> Option<bool> {
        let waiters = self.waiters.try_lock()?;
        Some(waiters.iter().any(|waiter| waiter.thread.id == thread_id))
    }

    /// Wake every waiter belonging to `process_id` with `EIO` and remove it from the queue.
    ///
    /// Used by process teardown for blocked sibling threads.  Removing without waking (the old
    /// `remove_process` behaviour) left the thread, its kernel stack and its local object
    /// references alive forever; waking it lets `block_current` observe `process.exiting` and
    /// run the normal exit path.  The wait-queue lock is released before the scheduler lock, so
    /// the usual queue-to-scheduler order is preserved.
    pub fn wake_process(&self, process_id: u64) -> usize {
        let mut count = 0usize;
        loop {
            let thread = {
                let mut waiters = self.waiters.lock();
                waiters
                    .iter()
                    .position(|waiter| waiter.thread.process.id == process_id)
                    .map(|index| waiters.remove(index).thread)
            };
            let Some(thread) = thread else {
                break;
            };
            if super::wake(thread, None, Errno::EIO.to_ret()) {
                count = count.saturating_add(1);
            }
        }
        count
    }

    /// Wake waiters of `process_id` on a private key in `[start, end)` with `EFAULT`.
    pub fn wake_private_range(&self, process_id: u64, start: u64, end: u64) -> usize {
        let mut count = 0usize;
        loop {
            let waiter = {
                let mut waiters = self.waiters.lock();
                waiters
                    .iter()
                    .position(|waiter| {
                        waiter.space == process_id
                            && waiter.thread.process.id == process_id
                            && waiter.addr >= start
                            && waiter.addr < end
                    })
                    .map(|index| waiters.remove(index).thread)
            };
            let Some(thread) = waiter else {
                break;
            };
            if super::wake(thread, None, Errno::EFAULT.to_ret()) {
                count = count.saturating_add(1);
            }
        }
        count
    }

    /// Wake waiters of `process_id` on a shared object/offset range with `EFAULT`.
    pub fn wake_shared_range(
        &self,
        process_id: u64,
        object_space: u64,
        start: u64,
        end: u64,
    ) -> usize {
        let mut count = 0usize;
        loop {
            let waiter = {
                let mut waiters = self.waiters.lock();
                waiters
                    .iter()
                    .position(|waiter| {
                        waiter.space == object_space
                            && waiter.thread.process.id == process_id
                            && waiter.addr >= start
                            && waiter.addr < end
                    })
                    .map(|index| waiters.remove(index).thread)
            };
            let Some(thread) = waiter else {
                break;
            };
            if super::wake(thread, None, Errno::EFAULT.to_ret()) {
                count = count.saturating_add(1);
            }
        }
        count
    }

    /// Number of waiters (diagnostics).
    #[allow(dead_code)] // diagnostics
    pub fn len(&self) -> usize {
        self.waiters.lock().len()
    }
}

impl Drop for WaitQueue {
    fn drop(&mut self) {
        let _guard = QUEUE_LOCK.lock();
        if !self.listed.swap(false, Ordering::AcqRel) {
            return;
        }
        let target = self as *const WaitQueue as *mut WaitQueue;
        let mut previous: *mut WaitQueue = ptr::null_mut();
        let mut current = QUEUE_HEAD.load(Ordering::Acquire);
        while !current.is_null() {
            // SAFETY: every listed queue is live under the registry lock.
            let next = unsafe { (*current).next.load(Ordering::Acquire) };
            if current == target {
                if previous.is_null() {
                    QUEUE_HEAD.store(next, Ordering::Release);
                } else {
                    // SAFETY: `previous` is a live listed queue.
                    unsafe { (*previous).next.store(next, Ordering::Release) };
                }
                break;
            }
            previous = current;
            current = next;
        }
    }
}

impl Default for WaitQueue {
    fn default() -> Self {
        Self::new()
    }
}
