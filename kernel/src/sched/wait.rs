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
}

impl WaitQueue {
    /// Create an empty queue.
    pub const fn new() -> Self {
        Self {
            waiters: IrqSpinLock::new(Vec::new()),
        }
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

impl Default for WaitQueue {
    fn default() -> Self {
        Self::new()
    }
}
