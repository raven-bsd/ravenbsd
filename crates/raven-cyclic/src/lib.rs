// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// Immediate cancellation ported from DragonFlyBSD sys/kern/kern_timeout.c
// `callout_stop`/`_callout_cancel_quick` and sys/kern/kern_synch.c `_tsleep_remove`
// @ 94448f622a06cde90ba42d688e9cab2448ad9ab3 (3-clause BSD; The DragonFly Project).
// Rust reimplementation; no C expression copied.

//! Pure per-CPU deadline timer core.
//!
//! One [`Cyclics`] holds the timers of a single CPU as a binary min-heap keyed by the absolute
//! fire deadline, plus a [`Backend`] that programs the hardware timer and reads the current time.
//! It models the next-deadline rule that makes arbitrary-resolution timers and tickless idle
//! possible without a periodic tick: a timer fires when its deadline is due, the backend is
//! reprogrammed to the earliest remaining deadline, and an idle CPU with no near deadline halts
//! instead of ticking.
//!
//! # Model
//!
//! * [`Cyclics::add`] inserts a cyclic and reprograms the backend if the new deadline is earlier
//!   than the one already programmed.
//! * [`Cyclics::cancel`] unlinks the timer immediately and reprograms the backend, following the
//!   DragonFly callout/sleepqueue stop path. Cancelling an unknown or already-cancelled id is a
//!   no-op.
//! * [`Cyclics::on_interrupt`] fires every due cyclic in deadline order (equal
//!   deadlines fire in id order), re-inserting periodic cyclics at `when + interval`, then
//!   reprograms the backend. Firing is the "ordered merge" of `R-TIM-10`: every deadline reached
//!   since the previous interrupt is drained in one pass, so nearby deadlines coalesce.
//! * The fire path never allocates: it only pops due entries and re-pushes periodic ones, which
//!   keeps the backing vector within its existing capacity.
//!
//! The `level` (interrupt vs thread context) and `slack_ns` (coalescing hint) are carried on each
//! cyclic for the kernel integration (`L4`); this crate owns the pure deadline ordering, not the
//! handler-dispatch or the hardware.

#![no_std]
#![forbid(unsafe_code)]

extern crate alloc;

use alloc::vec::Vec;

/// A stable handle to an inserted cyclic.
pub type CyclicId = u64;

/// Which context a cyclic's handler runs in (used by the kernel integration, `R-TIM-5`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Level {
    /// Runs on the per-CPU kernel thread; may block or take a mutex.
    Low,
    /// Runs on the per-CPU kernel thread with default priority.
    Normal,
    /// Runs in interrupt context; must not block or allocate.
    High,
}

/// Programs the next interrupt and reads the current time (the kernel-side hardware seam).
pub trait Backend {
    /// Program the timer to fire at `deadline_ns`, or disarm it with `None`.
    fn program(&mut self, deadline_ns: Option<u64>);
    /// The current time in nanoseconds (monotonic).
    fn now_ns(&self) -> u64;
}

/// A cyclic's specification.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CyclicSpec {
    /// Absolute fire deadline in nanoseconds.
    pub when: u64,
    /// Repeat period in nanoseconds; `0` is a one-shot.
    pub interval: u64,
    /// Handler context for the kernel integration.
    pub level: Level,
    /// Coalescing slack in nanoseconds (`0` means no slack).
    pub slack_ns: u64,
}

/// One heap entry.
#[derive(Clone, Copy, Debug)]
struct Entry {
    id: CyclicId,
    when: u64,
    interval: u64,
    level: Level,
    slack_ns: u64,
}

impl Entry {
    fn from_spec(id: CyclicId, spec: CyclicSpec) -> Self {
        Entry {
            id,
            when: spec.when,
            interval: spec.interval,
            level: spec.level,
            slack_ns: spec.slack_ns,
        }
    }

    /// Heap order: earliest deadline, then lowest id.
    fn less(&self, other: &Entry) -> bool {
        (self.when, self.id) < (other.when, other.id)
    }
}

/// Per-CPU deadline timer heap plus its backend.
pub struct Cyclics<B: Backend> {
    heap: Vec<Entry>,
    backend: B,
    next_id: CyclicId,
    programmed: Option<u64>,
}

impl<B: Backend> Cyclics<B> {
    /// Create an empty cyclic set over `backend`.
    #[must_use]
    pub fn new(backend: B) -> Self {
        Cyclics {
            heap: Vec::new(),
            backend,
            next_id: 0,
            programmed: None,
        }
    }

    /// Insert a cyclic and return its handle.
    pub fn add(&mut self, spec: CyclicSpec) -> CyclicId {
        let id = self.next_id;
        self.next_id = self.next_id.wrapping_add(1);
        let entry = Entry::from_spec(id, spec);
        self.heap.push(entry);
        self.sift_up(self.heap.len().saturating_sub(1));
        self.reprogram();
        id
    }

    /// Cancel a cyclic and unlink it from the heap immediately.
    ///
    /// Returns `false` when the id is unknown or already cancelled. Removing the entry, rather
    /// than marking it, matches DragonFly's `callout_stop`/`_callout_cancel_quick` and the
    /// sleepqueue `_tsleep_remove`, and prevents a cancelled last timer from being retained after
    /// the backend is disarmed. This is idempotent and never panics.
    pub fn cancel(&mut self, id: CyclicId) -> bool {
        let Some(index) = self.heap.iter().position(|entry| entry.id == id) else {
            return false;
        };
        let last = self.heap.len().saturating_sub(1);
        self.heap.swap_remove(index);
        if index < last {
            let parent = index.saturating_sub(1) / 2;
            if index > 0 && self.heap[index].less(&self.heap[parent]) {
                self.sift_up(index);
            } else {
                self.sift_down(index);
            }
        }
        self.reprogram();
        true
    }

    /// Fire every due cyclic and reprogram the backend.
    ///
    /// `now` is the current monotonic time (from [`Backend::now_ns`]); `fire` is called once per
    /// fired cyclic, in deadline order (equal deadlines in id order). This path performs no
    /// allocation.
    pub fn on_interrupt(&mut self, now: u64, mut fire: impl FnMut(CyclicId)) {
        self.on_interrupt_with(now, |id, _spec| fire(id));
    }

    /// Fire every due cyclic and pass its specification to the callback.
    ///
    /// This is the level-aware form of [`Cyclics::on_interrupt`]: the entry is popped before the
    /// callback runs, so a caller that dispatches high/lock/low handlers cannot recover the level
    /// with [`Cyclics::spec`] afterwards. The callback receives the same [`CyclicSpec`] that was
    /// live at the firing deadline (including the current `when` for a periodic cyclic). The
    /// callback runs in deadline order; equal deadlines fire in id order. This path performs no
    /// allocation.
    pub fn on_interrupt_with(&mut self, now: u64, mut fire: impl FnMut(CyclicId, CyclicSpec)) {
        while let Some(root) = self.heap.first().copied() {
            if root.when > now {
                break;
            }
            self.heap_pop();
            let spec = CyclicSpec {
                when: root.when,
                interval: root.interval,
                level: root.level,
                slack_ns: root.slack_ns,
            };
            fire(root.id, spec);
            if root.interval != 0 {
                let mut next = root;
                next.when = next.when.saturating_add(next.interval);
                self.heap.push(next);
                self.sift_up(self.heap.len().saturating_sub(1));
            }
        }
        self.reprogram();
    }

    /// The earliest deadline, or `None` when nothing is pending.
    #[must_use]
    pub fn next_deadline(&self) -> Option<u64> {
        self.heap.first().map(|entry| entry.when)
    }

    /// The live cyclic's current specification, or `None` when it is unknown or cancelled.
    #[must_use]
    pub fn spec(&self, id: CyclicId) -> Option<CyclicSpec> {
        self.heap
            .iter()
            .find(|entry| entry.id == id)
            .map(|entry| CyclicSpec {
                when: entry.when,
                interval: entry.interval,
                level: entry.level,
                slack_ns: entry.slack_ns,
            })
    }

    /// Number of live timers in the heap.
    #[must_use]
    pub fn len(&self) -> usize {
        self.heap.len()
    }

    /// True when the heap has no entries at all.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.heap.is_empty()
    }

    /// The backend (for diagnostics and the kernel integration).
    #[must_use]
    pub fn backend(&self) -> &B {
        &self.backend
    }

    /// Mutable backend access.
    #[must_use]
    pub fn backend_mut(&mut self) -> &mut B {
        &mut self.backend
    }

    /// Reprogram the backend if the next deadline differs from the last programmed one.
    fn reprogram(&mut self) {
        let next = self.next_deadline();
        if next != self.programmed {
            self.programmed = next;
            self.backend.program(next);
        }
    }

    /// Remove and return the smallest entry.
    fn heap_pop(&mut self) -> Option<Entry> {
        if self.heap.is_empty() {
            return None;
        }
        let last = self.heap.len().saturating_sub(1);
        let root = self.heap.swap_remove(0);
        if last > 0 {
            self.sift_down(0);
        }
        Some(root)
    }

    fn sift_up(&mut self, mut index: usize) {
        while index > 0 {
            let parent = index.saturating_sub(1) / 2;
            let less = match (self.heap.get(index), self.heap.get(parent)) {
                (Some(child), Some(parent_entry)) => child.less(parent_entry),
                _ => false,
            };
            if !less {
                break;
            }
            self.heap.swap(index, parent);
            index = parent;
        }
    }

    fn sift_down(&mut self, mut index: usize) {
        loop {
            let left = index.saturating_mul(2).saturating_add(1);
            let right = left.saturating_add(1);
            let mut smallest = index;
            for candidate in [left, right] {
                let less = match (self.heap.get(candidate), self.heap.get(smallest)) {
                    (Some(child), Some(current)) => child.less(current),
                    _ => false,
                };
                if less {
                    smallest = candidate;
                }
            }
            if smallest == index {
                break;
            }
            self.heap.swap(index, smallest);
            index = smallest;
        }
    }
}

#[cfg(test)]
#[path = "lib_ut.rs"]
mod tests;
