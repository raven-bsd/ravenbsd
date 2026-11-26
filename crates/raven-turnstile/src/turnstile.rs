// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The turnstile registry: wait queues keyed by lock id plus priority inheritance.

use alloc::collections::{BTreeMap, BTreeSet};
use alloc::vec::Vec;
use core::hash::{Hash, Hasher};

/// Identifier of a thread in the model.
pub type ThreadId = u32;

/// Identifier of the lock/object a turnstile is keyed by.
pub type ObjectId = u32;

/// A priority level: `0` is the highest, `255` the lowest (as in `raven-sched-core`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Priority(u8);

impl Priority {
    /// The most urgent priority.
    pub const HIGHEST: Priority = Priority(0);
    /// The least urgent priority.
    pub const LOWEST: Priority = Priority(u8::MAX);

    /// Build a priority from a raw level.
    #[must_use]
    pub const fn new(raw: u8) -> Priority {
        Priority(raw)
    }

    /// The raw level (`0` is the highest).
    #[must_use]
    pub const fn raw(self) -> u8 {
        self.0
    }
}

/// The effect an operation had on an owner's effective priority.
///
/// `before`/`after` are the owner's effective priority on either side of the mutation; `changed()`
/// reports whether the mutation actually moved it. A no-op mutation still yields an effect with
/// `before == after`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct InheritEffect {
    /// The owner whose effective priority is described.
    pub owner: ThreadId,
    /// Effective priority before the mutation.
    pub before: Priority,
    /// Effective priority after the mutation.
    pub after: Priority,
}

impl InheritEffect {
    /// Whether the owner's effective priority actually changed.
    #[must_use]
    pub fn changed(self) -> bool {
        self.before != self.after
    }
}

/// The result of a cancellation.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct CancelOutcome {
    /// Whether a waiter was removed.
    pub removed: bool,
    /// The owner's inheritance effect when a waiter was removed, or `None` otherwise.
    pub effect: Option<InheritEffect>,
}

/// Per-object lock statistics, compiled only under the `lockstat` feature.
///
/// These are the counters the kernel `lockstat` probes will publish (L5); the pure crate grows
/// them now so the kernel side is plumbing. Every counter is saturating: it never wraps silently.
#[cfg(feature = "lockstat")]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct LockStats {
    /// Number of threads that blocked on this object.
    pub wait_entries: u64,
    /// Number of uncontended acquisitions.
    pub immediate_acquisitions: u64,
    /// Number of threads woken (`wake_one` + `wake_all`).
    pub wakes: u64,
    /// Number of waiters removed by [`Turnstiles::cancel`].
    pub cancels: u64,
    /// Reserved for the kernel timeout path; always `0` here (the pure model has no timeout).
    pub timeouts: u64,
    /// The largest number of simultaneous waiters observed.
    pub max_queue_depth: u64,
    /// The longest inheritance (wait-for) chain observed on this object, in hops.
    pub max_inheritance_depth: u64,
    /// Number of times an operation changed an owner's effective priority.
    pub inherited_priority_changes: u64,
}

/// Why a turnstile operation was rejected.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Error {
    /// The thread is not registered with [`Turnstiles::register`].
    UnknownThread,
    /// The thread is already registered.
    DuplicateThread,
    /// The object is already owned by `owner`.
    AlreadyOwned {
        /// The thread that already owns the object.
        owner: ThreadId,
    },
    /// The thread already waits on this object.
    AlreadyWaiting {
        /// The object the thread already waits on.
        object: ObjectId,
    },
    /// The thread already waits on `object` (a different one).
    BlockedElsewhere {
        /// The object the thread already waits on.
        object: ObjectId,
    },
    /// The thread tried to block on an object it owns.
    OwnsObject,
    /// The object has no owner, so nobody can block on it or be boosted by it.
    NotOwned,
    /// [`Turnstiles::unboost`] was called while the object still has waiters.
    WaitersRemain,
}

/// A violated turnstile invariant, returned by [`Turnstiles::check_invariants`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum InvariantViolation {
    /// An owner also appears in its own waiter list.
    OwnerAlsoWaiter,
    /// The same thread appears twice in one waiter list.
    DuplicateWaiter,
    /// A waiter list is not sorted by `(priority, arrival)`.
    WaiterOrdering,
    /// A thread waits on more than one object.
    ThreadBlockedTwice,
    /// A turnstile's stored inheritance link does not match its highest waiter.
    PiMismatch,
    /// The stored effective-priority cache diverges from the least fixed point.
    EffectiveMismatch,
    /// An effective priority is *worse* (higher) than the thread's base priority.
    EffectiveWorseThanBase,
}

/// One blocked thread and the priority it blocked at.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct Waiter {
    thread: ThreadId,
    priority: Priority,
    arrival: u64,
}

/// One turnstile: the wait queue plus the inheritance link to its owner.
#[derive(Clone, Debug, Default)]
struct Turnstile {
    owner: Option<ThreadId>,
    waiters: Vec<Waiter>,
    pi: Option<Priority>,
    next_arrival: u64,
}

impl PartialEq for Turnstile {
    fn eq(&self, other: &Self) -> bool {
        self.owner == other.owner
            && self.waiters == other.waiters
            && self.pi == other.pi
            && self.next_arrival == other.next_arrival
    }
}

impl Eq for Turnstile {}

impl core::hash::Hash for Turnstile {
    fn hash<H: core::hash::Hasher>(&self, state: &mut H) {
        self.owner.hash(state);
        self.waiters.hash(state);
        self.pi.hash(state);
        self.next_arrival.hash(state);
    }
}

/// The set of turnstiles, keyed by object id, plus per-thread base and effective priorities.
///
/// Every mutation recomputes the effective priority of every thread as the *least fixed point* of
/// `eff(t) = min(base(t), { eff(w) | w waits on a lock owned by t })`. Because each thread's
/// priority only ever rises (its numeric value falls) toward `0`, the fixed point is unique and is
/// reached by iterating to stability. This is the transitive inheritance of `R-LCK-6`; the kernel
/// glue may replace the recomputation with incremental propagation that yields the same value.
#[derive(Clone, Debug, Default)]
pub struct Turnstiles {
    threads: BTreeMap<ThreadId, Priority>,
    effective: BTreeMap<ThreadId, Priority>,
    objects: BTreeMap<ObjectId, Turnstile>,
    /// Per-lock statistics keyed by object id. They outlive the transient turnstile: an idle
    /// turnstile is reclaimed (FreeBSD `turnstile_free`), while the counters belong to the lock
    /// object (FreeBSD `lock_profile_object` keyed by `lock_object`; NetBSD `lockdebug` is
    /// allocated at lock init and freed at lock destroy). Excluded from equality and hashing so
    /// the exhaustive model check still deduplicates on the logical state.
    #[cfg(feature = "lockstat")]
    stats: BTreeMap<ObjectId, LockStats>,
}

impl PartialEq for Turnstiles {
    fn eq(&self, other: &Self) -> bool {
        self.threads == other.threads
            && self.effective == other.effective
            && self.objects == other.objects
    }
}

impl Eq for Turnstiles {}

impl Hash for Turnstiles {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.threads.hash(state);
        self.effective.hash(state);
        self.objects.hash(state);
    }
}

impl Turnstiles {
    /// Create an empty registry.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a thread with its base priority.
    pub fn register(&mut self, thread: ThreadId, base: Priority) -> Result<(), Error> {
        if self.threads.contains_key(&thread) {
            return Err(Error::DuplicateThread);
        }
        self.threads.insert(thread, base);
        self.effective.insert(thread, base);
        Ok(())
    }

    /// Unregister a thread that is leaving (FreeBSD `thread_fini`, NetBSD `lwp_dtor`).
    ///
    /// Drops the thread's base/effective entries, its wait-queue entry and any object it owns.
    /// The removal is allocation-free: the effective fixed point is recomputed in place for the
    /// remaining registered threads, so an exiting thread cannot leave a map entry behind or pin
    /// the 3072-byte class. An object whose owner exits with waiters stays behind (with no owner)
    /// until those waiters leave; a reused thread id never inherits it.
    pub fn unregister(&mut self, thread: ThreadId) -> bool {
        let was_registered = self.threads.remove(&thread).is_some();
        self.effective.remove(&thread);
        for turnstile in self.objects.values_mut() {
            turnstile.waiters.retain(|waiter| waiter.thread != thread);
            if turnstile.owner == Some(thread) {
                turnstile.owner = None;
            }
            turnstile.pi = turnstile.waiters.first().map(|waiter| waiter.priority);
        }
        self.objects
            .retain(|_, turnstile| turnstile.owner.is_some() || !turnstile.waiters.is_empty());
        self.refresh_without_alloc();
        was_registered
    }

    /// Recompute every effective priority in place, then re-sort the waiter lists.
    ///
    /// [`Turnstiles::refresh`] clones the base map; the thread-exit path cannot allocate. Every
    /// registered thread has an `effective` entry (`register` and `unregister` maintain the pair),
    /// so resetting in place is exact and the fixed point is the same as `compute_effective`.
    fn refresh_without_alloc(&mut self) {
        for (thread, base) in &self.threads {
            if let Some(effective) = self.effective.get_mut(thread) {
                *effective = *base;
            }
        }
        loop {
            let mut changed = false;
            for turnstile in self.objects.values() {
                let Some(owner) = turnstile.owner else {
                    continue;
                };
                let mut pi = Priority::LOWEST;
                for waiter in &turnstile.waiters {
                    if let Some(priority) = self.effective.get(&waiter.thread).copied() {
                        pi = pi.min(priority);
                    }
                }
                if let Some(current) = self.effective.get_mut(&owner)
                    && pi < *current
                {
                    *current = pi;
                    changed = true;
                }
            }
            if !changed {
                break;
            }
        }
        for turnstile in self.objects.values_mut() {
            for waiter in &mut turnstile.waiters {
                if let Some(priority) = self.effective.get(&waiter.thread).copied() {
                    waiter.priority = priority;
                }
            }
            turnstile
                .waiters
                .sort_by_key(|waiter| (waiter.priority, waiter.arrival));
            turnstile.pi = turnstile.waiters.first().map(|waiter| waiter.priority);
        }
    }

    /// The thread's base priority, if it is registered.
    #[must_use]
    pub fn base_priority(&self, thread: ThreadId) -> Option<Priority> {
        self.threads.get(&thread).copied()
    }

    /// The thread's current effective priority, if it is registered.
    #[must_use]
    pub fn effective_priority(&self, thread: ThreadId) -> Option<Priority> {
        self.effective.get(&thread).copied()
    }

    /// Take ownership of `object` on behalf of `thread`.
    pub fn acquire(&mut self, object: ObjectId, thread: ThreadId) -> Result<(), Error> {
        if !self.threads.contains_key(&thread) {
            return Err(Error::UnknownThread);
        }
        if let Some(owner) = self.objects.get(&object).and_then(|t| t.owner) {
            return Err(Error::AlreadyOwned { owner });
        }
        self.objects.entry(object).or_default().owner = Some(thread);
        #[cfg(feature = "lockstat")]
        self.stat_acquire(object);
        self.refresh();
        Ok(())
    }

    /// Block `thread` on `object`'s turnstile at its current effective priority.
    ///
    /// The object must have an owner (a turnstile only blocks on a held lock). The caller thread
    /// must not own the object and must not already wait on any object. On success the owner's
    /// effective priority may rise, reported by the returned [`InheritEffect`].
    pub fn block(&mut self, object: ObjectId, thread: ThreadId) -> Result<InheritEffect, Error> {
        if !self.threads.contains_key(&thread) {
            return Err(Error::UnknownThread);
        }
        if self.owner(object) == Some(thread) {
            return Err(Error::OwnsObject);
        }
        if let Some(other) = self.waiting_on(thread) {
            return if other == object {
                Err(Error::AlreadyWaiting { object })
            } else {
                Err(Error::BlockedElsewhere { object: other })
            };
        }
        let (owner, before) = self.owner_snapshot(object).ok_or(Error::NotOwned)?;
        let waiter_priority = self
            .effective
            .get(&thread)
            .copied()
            .unwrap_or_else(|| self.threads[&thread]);
        let turnstile = self.objects.entry(object).or_default();
        let arrival = turnstile.next_arrival;
        turnstile.next_arrival = arrival.saturating_add(1);
        turnstile.waiters.push(Waiter {
            thread,
            priority: waiter_priority,
            arrival,
        });
        self.refresh();
        let after = self
            .effective
            .get(&owner)
            .copied()
            .ok_or(Error::UnknownThread)?;
        #[cfg(feature = "lockstat")]
        self.stat_block(object, owner, before != after);
        Ok(InheritEffect {
            owner,
            before,
            after,
        })
    }

    /// Wake the highest-priority waiter and hand it ownership (direct handoff).
    ///
    /// Returns the woken thread, or `None` when there was no waiter; in that case the object is
    /// released (its owner is cleared). A thread is woken at most once: after this returns a
    /// thread, it is no longer a waiter and cannot be woken again.
    #[must_use]
    pub fn wake_one(&mut self, object: ObjectId) -> Option<ThreadId> {
        let turnstile = self.objects.get_mut(&object)?;
        let woken = match turnstile.waiters.first().map(|w| w.thread) {
            Some(next) => {
                turnstile.waiters.remove(0);
                turnstile.owner = Some(next);
                Some(next)
            }
            None => {
                turnstile.owner = None;
                None
            }
        };
        self.refresh();
        #[cfg(feature = "lockstat")]
        if woken.is_some() {
            self.stat_wake(object, 1);
        }
        // FreeBSD `turnstile_free`/NetBSD `turnstile_free`: an idle turnstile does not outlive
        // the last owner/waiter. The next acquire recreates it from the object id.
        if self
            .objects
            .get(&object)
            .is_some_and(|t| t.owner.is_none() && t.waiters.is_empty())
        {
            self.objects.remove(&object);
        }
        woken
    }

    /// Release `object` and wake every waiter, in priority order, at most once each.
    #[must_use]
    pub fn wake_all(&mut self, object: ObjectId) -> Vec<ThreadId> {
        let woken: Vec<ThreadId> = match self.objects.get_mut(&object) {
            None => return Vec::new(),
            Some(turnstile) => {
                turnstile.owner = None;
                turnstile.waiters.drain(..).map(|w| w.thread).collect()
            }
        };
        #[cfg(feature = "lockstat")]
        self.stat_wake(object, woken.len() as u64);
        self.refresh();
        // Reclaim an idle turnstile after the last waiter drains.
        if self
            .objects
            .get(&object)
            .is_some_and(|t| t.owner.is_none() && t.waiters.is_empty())
        {
            self.objects.remove(&object);
        }
        woken
    }

    /// Cancel `thread`'s wait on `object` without waking it.
    ///
    /// A cancelled waiter is removed from the queue and never woken. The owner's effective
    /// priority is recomputed, so cancelling the last boosted waiter restores the owner.
    pub fn cancel(&mut self, object: ObjectId, thread: ThreadId) -> CancelOutcome {
        let before = self.owner_snapshot(object);
        let removed = match self.objects.get_mut(&object) {
            None => {
                return CancelOutcome {
                    removed: false,
                    effect: None,
                };
            }
            Some(turnstile) => match turnstile.waiters.iter().position(|w| w.thread == thread) {
                Some(pos) => {
                    turnstile.waiters.remove(pos);
                    true
                }
                None => false,
            },
        };
        self.refresh();
        let effect = if removed {
            before.and_then(|(owner, before)| {
                let after = self.effective.get(&owner).copied()?;
                Some(InheritEffect {
                    owner,
                    before,
                    after,
                })
            })
        } else {
            None
        };
        #[cfg(feature = "lockstat")]
        if removed {
            self.stat_cancel(object, effect.is_some_and(InheritEffect::changed));
        }
        // Reclaim an idle turnstile when a cancellation leaves no owner or waiter.
        if self
            .objects
            .get(&object)
            .is_some_and(|t| t.owner.is_none() && t.waiters.is_empty())
        {
            self.objects.remove(&object);
        }
        CancelOutcome { removed, effect }
    }

    /// Recompute and report the inheritance effect on `object`'s owner.
    ///
    /// Inheritance is maintained automatically by `block`/`wake`/`cancel`; this is the explicit
    /// re-evaluation point the kernel calls at the owner boundary. It is idempotent.
    pub fn boost(&mut self, object: ObjectId) -> Result<Option<InheritEffect>, Error> {
        let (owner, before) = self.owner_snapshot(object).ok_or(Error::NotOwned)?;
        self.refresh();
        let after = self
            .effective
            .get(&owner)
            .copied()
            .ok_or(Error::UnknownThread)?;
        #[cfg(feature = "lockstat")]
        if before != after {
            self.stat_priority_change(object);
        }
        Ok(Some(InheritEffect {
            owner,
            before,
            after,
        }))
    }

    /// Drop `object`'s inheritance contribution and report the owner's restored priority.
    ///
    /// This is the release-side counterpart of [`Turnstiles::boost`]: it is only legal once the
    /// object has no waiters (wake them first), and returns [`Error::WaitersRemain`] otherwise.
    pub fn unboost(&mut self, object: ObjectId) -> Result<Option<InheritEffect>, Error> {
        if self
            .objects
            .get(&object)
            .is_some_and(|t| !t.waiters.is_empty())
        {
            return Err(Error::WaitersRemain);
        }
        let (owner, before) = self.owner_snapshot(object).ok_or(Error::NotOwned)?;
        self.refresh();
        let after = self
            .effective
            .get(&owner)
            .copied()
            .ok_or(Error::UnknownThread)?;
        #[cfg(feature = "lockstat")]
        if before != after {
            self.stat_priority_change(object);
        }
        Ok(Some(InheritEffect {
            owner,
            before,
            after,
        }))
    }

    /// The owner of `object`, if any.
    #[must_use]
    pub fn owner(&self, object: ObjectId) -> Option<ThreadId> {
        self.objects.get(&object).and_then(|t| t.owner)
    }

    /// Whether `thread` owns `object`.
    #[must_use]
    pub fn is_owner(&self, object: ObjectId, thread: ThreadId) -> bool {
        self.owner(object) == Some(thread)
    }

    /// The waiters of `object`, highest priority first.
    #[must_use]
    pub fn waiters(&self, object: ObjectId) -> Vec<ThreadId> {
        self.objects
            .get(&object)
            .map(|t| t.waiters.iter().map(|w| w.thread).collect())
            .unwrap_or_default()
    }

    /// The number of waiters on `object`.
    #[must_use]
    pub fn waiter_count(&self, object: ObjectId) -> usize {
        self.objects.get(&object).map_or(0, |t| t.waiters.len())
    }

    /// Whether `thread` waits on `object`.
    #[must_use]
    pub fn is_waiting(&self, object: ObjectId, thread: ThreadId) -> bool {
        self.objects
            .get(&object)
            .is_some_and(|t| t.waiters.iter().any(|w| w.thread == thread))
    }

    /// The object `thread` waits on, if any.
    #[must_use]
    pub fn waiting_on(&self, thread: ThreadId) -> Option<ObjectId> {
        self.objects
            .iter()
            .find(|(_, t)| t.waiters.iter().any(|w| w.thread == thread))
            .map(|(object, _)| *object)
    }

    /// The objects owned by `thread`, in key order.
    #[must_use]
    pub fn owned_objects(&self, thread: ThreadId) -> Vec<ObjectId> {
        self.objects
            .iter()
            .filter(|(_, t)| t.owner == Some(thread))
            .map(|(object, _)| *object)
            .collect()
    }

    /// All registered threads and their base priorities (for the IPC donation layer).
    pub(crate) fn threads(&self) -> impl Iterator<Item = (ThreadId, Priority)> + '_ {
        self.threads.iter().map(|(thread, base)| (*thread, *base))
    }

    /// All object ids that currently exist (for the IPC donation layer).
    pub(crate) fn objects(&self) -> impl Iterator<Item = ObjectId> + '_ {
        self.objects.keys().copied()
    }

    /// Visit every waiter on `object` without allocating (the donation teardown path is
    /// stack- and allocation-bounded).
    pub(crate) fn for_each_waiter(&self, object: ObjectId, mut visit: impl FnMut(ThreadId)) {
        if let Some(turnstile) = self.objects.get(&object) {
            for waiter in &turnstile.waiters {
                visit(waiter.thread);
            }
        }
    }

    /// Check every invariant the model promises.
    ///
    /// Returns the first violation. Used by the exhaustive model check and by the kernel glue in
    /// debug builds; it never panics and reads only the current state.
    pub fn check_invariants(&self) -> Result<(), InvariantViolation> {
        for turnstile in self.objects.values() {
            let mut seen = BTreeSet::new();
            let mut prev: Option<(Priority, u64)> = None;
            for waiter in &turnstile.waiters {
                if Some(waiter.thread) == turnstile.owner {
                    return Err(InvariantViolation::OwnerAlsoWaiter);
                }
                if !seen.insert(waiter.thread) {
                    return Err(InvariantViolation::DuplicateWaiter);
                }
                if let Some((p_priority, p_arrival)) = prev {
                    if (waiter.priority, waiter.arrival) < (p_priority, p_arrival) {
                        return Err(InvariantViolation::WaiterOrdering);
                    }
                    if waiter.arrival == p_arrival {
                        return Err(InvariantViolation::WaiterOrdering);
                    }
                }
                prev = Some((waiter.priority, waiter.arrival));
            }
            let expected_pi = turnstile.waiters.first().map(|w| w.priority);
            if turnstile.pi != expected_pi {
                return Err(InvariantViolation::PiMismatch);
            }
        }
        let mut waiting = BTreeSet::new();
        for turnstile in self.objects.values() {
            for waiter in &turnstile.waiters {
                if !waiting.insert(waiter.thread) {
                    return Err(InvariantViolation::ThreadBlockedTwice);
                }
            }
        }
        if self.compute_effective() != self.effective {
            return Err(InvariantViolation::EffectiveMismatch);
        }
        for (thread, base) in &self.threads {
            let effective = self.effective.get(thread).copied().unwrap_or(*base);
            if effective > *base {
                return Err(InvariantViolation::EffectiveWorseThanBase);
            }
        }
        Ok(())
    }

    /// The current effective priority of `object`'s owner, with its thread id.
    fn owner_snapshot(&self, object: ObjectId) -> Option<(ThreadId, Priority)> {
        let owner = self.objects.get(&object)?.owner?;
        let priority = self.effective.get(&owner).copied()?;
        Some((owner, priority))
    }

    /// Compute the least fixed point of effective priority from the base priorities.
    fn compute_effective(&self) -> BTreeMap<ThreadId, Priority> {
        let mut effective = self.threads.clone();
        loop {
            let mut changed = false;
            for turnstile in self.objects.values() {
                let Some(owner) = turnstile.owner else {
                    continue;
                };
                let mut pi = Priority::LOWEST;
                for waiter in &turnstile.waiters {
                    if let Some(priority) = effective.get(&waiter.thread).copied() {
                        pi = pi.min(priority);
                    }
                }
                if let Some(current) = effective.get_mut(&owner)
                    && pi < *current
                {
                    *current = pi;
                    changed = true;
                }
            }
            if !changed {
                return effective;
            }
        }
    }

    /// Recompute every effective priority, then re-sort waiter lists and refresh the stored
    /// inheritance links to match.
    fn refresh(&mut self) {
        let effective = self.compute_effective();
        for turnstile in self.objects.values_mut() {
            for waiter in &mut turnstile.waiters {
                if let Some(priority) = effective.get(&waiter.thread).copied() {
                    waiter.priority = priority;
                }
            }
            turnstile
                .waiters
                .sort_by_key(|waiter| (waiter.priority, waiter.arrival));
            turnstile.pi = turnstile.waiters.first().map(|waiter| waiter.priority);
        }
        self.effective = effective;
    }
}

/// Upper bound on the per-object lockstat records.
///
/// FreeBSD's `LPROF_CACHE_SIZE` profiling cache is fixed too: when it is exhausted it rejects
/// new objects (`lock_prof_rejected`) instead of growing without bound. Kernel locks that are
/// created and destroyed per process (the `Process` fields are `Mutex` instances) must not turn
/// the diagnostic into a new leak; existing records stay valid and new objects are simply not
/// profiled once the cap is reached.
#[cfg(feature = "lockstat")]
const MAX_STATS_OBJECTS: usize = 4096;

#[cfg(feature = "lockstat")]
impl Turnstiles {
    /// Snapshot the lock statistics for `object` (all zero when it has never been touched).
    #[must_use]
    pub fn stats(&self, object: ObjectId) -> LockStats {
        self.stats.get(&object).copied().unwrap_or_default()
    }

    /// The object ids that have recorded statistics.
    pub fn stats_objects(&self) -> impl Iterator<Item = ObjectId> + '_ {
        self.stats.keys().copied()
    }

    fn stat_entry(&mut self, object: ObjectId) -> Option<&mut LockStats> {
        if self.stats.len() >= MAX_STATS_OBJECTS && !self.stats.contains_key(&object) {
            return None;
        }
        Some(self.stats.entry(object).or_default())
    }

    fn stat_acquire(&mut self, object: ObjectId) {
        let Some(stats) = self.stat_entry(object) else {
            return;
        };
        stats.immediate_acquisitions = stats.immediate_acquisitions.saturating_add(1);
    }

    fn stat_block(&mut self, object: ObjectId, owner: ThreadId, changed: bool) {
        let depth = self.waiter_count(object) as u64;
        let chain = 1u64.saturating_add(self.chain_length(owner) as u64);
        let Some(stats) = self.stat_entry(object) else {
            return;
        };
        stats.wait_entries = stats.wait_entries.saturating_add(1);
        stats.max_queue_depth = stats.max_queue_depth.max(depth);
        stats.max_inheritance_depth = stats.max_inheritance_depth.max(chain);
        if changed {
            stats.inherited_priority_changes = stats.inherited_priority_changes.saturating_add(1);
        }
    }

    fn stat_wake(&mut self, object: ObjectId, count: u64) {
        let Some(stats) = self.stat_entry(object) else {
            return;
        };
        stats.wakes = stats.wakes.saturating_add(count);
    }

    fn stat_cancel(&mut self, object: ObjectId, changed: bool) {
        let Some(stats) = self.stat_entry(object) else {
            return;
        };
        stats.cancels = stats.cancels.saturating_add(1);
        if changed {
            stats.inherited_priority_changes = stats.inherited_priority_changes.saturating_add(1);
        }
    }

    fn stat_priority_change(&mut self, object: ObjectId) {
        let Some(stats) = self.stat_entry(object) else {
            return;
        };
        stats.inherited_priority_changes = stats.inherited_priority_changes.saturating_add(1);
    }

    /// The wait-for chain length from `thread`: hops along "thread waits on `O`, owned by `P`; `P`
    /// waits on `O2`, owned by `Q`; ..." until a thread that owns but does not wait.
    fn chain_length(&self, thread: ThreadId) -> usize {
        let mut length = 0usize;
        let mut current = thread;
        let mut visited = BTreeSet::new();
        while visited.insert(current) {
            let Some(object) = self.waiting_on(current) else {
                break;
            };
            length = length.saturating_add(1);
            let Some(owner) = self.owner(object) else {
                break;
            };
            current = owner;
        }
        length
    }
}

#[cfg(test)]
#[path = "turnstile_ut.rs"]
mod tests;
