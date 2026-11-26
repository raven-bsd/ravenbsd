// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! IPC priority donation graph layered on the turnstile.
//!
//! A turnstile propagates priority through a lock wait queue. An IPC call adds a second edge: a
//! client blocked on a server reply donates its priority to the server, and that boost must flow
//! through any lock the server then waits on. This module models both edge kinds and computes each
//! thread's effective priority as the *least fixed point* over them:
//!
//! `eff(t) = min(base(t), { eff(w) | w waits on a lock owned by t }, { eff(c) | c has a call to t })`
//!
//! The lookup path (`effective_priority`) is allocation-free (a cached map); mutations recompute
//! the fixed point. Call edges are `client -> server` with at most one outstanding call per
//! client; completion, cancellation, client death and server death remove exactly the right edges.

use alloc::collections::BTreeMap;
use alloc::vec::Vec;

use super::{CancelOutcome, Error, InheritEffect, ObjectId, Priority, ThreadId, Turnstiles};

/// Why a donation operation was rejected.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum DonationError {
    /// A thread id is not registered.
    UnknownThread,
    /// The client already has an outstanding call.
    DuplicateCall,
    /// No matching outstanding call exists to end or cancel.
    StaleCall,
}

/// A violated donation invariant, returned by [`Donation::check_invariants`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum DonationViolation {
    /// A call edge references an unregistered thread.
    DanglingCall,
    /// The cached effective priority diverges from the least fixed point.
    EffectiveMismatch,
    /// A thread with no donor (call or lock) is not at its base priority.
    BaseNotRestored,
}

/// Maximum number of threads whose cached effective priority one allocation-free edge removal
/// recomputes. Larger dependent graphs are marked dirty and refreshed at the next allocating
/// mutation (which is not a teardown/fault context); the bound keeps the exit path stack-bounded.
pub const MAX_NO_ALLOC_AFFECTED: usize = 64;

/// A turnstile registry extended with IPC call edges for priority donation.
pub struct Donation {
    turnstiles: Turnstiles,
    calls: BTreeMap<ThreadId, ThreadId>,
    effective: BTreeMap<ThreadId, Priority>,
    max_depth: usize,
    dirty: bool,
}

impl Donation {
    /// Create an empty donation graph.
    #[must_use]
    pub fn new() -> Self {
        Donation {
            turnstiles: Turnstiles::new(),
            calls: BTreeMap::new(),
            effective: BTreeMap::new(),
            max_depth: 0,
            dirty: false,
        }
    }

    /// Register a thread with its base priority.
    pub fn register(&mut self, thread: ThreadId, base: Priority) -> Result<(), Error> {
        self.turnstiles.register(thread, base)?;
        self.refresh();
        Ok(())
    }

    /// Unregister a thread that is leaving (FreeBSD `thread_fini`, NetBSD `lwp_dtor`).
    ///
    /// Removes every call edge where the thread is the client or the server, drops its turnstile
    /// registration (base, effective, wait queue and owned objects) and recomputes the remaining
    /// fixed point in place. The exit path does not allocate, so the 3072-byte graph entries are
    /// released rather than carried into the next thread that reuses the id.
    pub fn unregister(&mut self, thread: ThreadId) -> bool {
        self.calls.remove(&thread);
        self.calls.retain(|_, server| *server != thread);
        let was_registered = self.turnstiles.unregister(thread);
        self.effective.remove(&thread);
        self.refresh_without_alloc();
        was_registered
    }

    /// The thread's base priority.
    #[must_use]
    pub fn base_priority(&self, thread: ThreadId) -> Option<Priority> {
        self.turnstiles.base_priority(thread)
    }

    /// The thread's combined effective priority (base, lock waiters and call clients, transitive).
    ///
    /// Allocation-free: a cached map lookup.
    #[must_use]
    pub fn effective_priority(&self, thread: ThreadId) -> Option<Priority> {
        self.effective.get(&thread).copied()
    }

    /// Open a call edge: `client` is blocked on a reply from `server`.
    pub fn begin_call(&mut self, client: ThreadId, server: ThreadId) -> Result<(), DonationError> {
        if !self.is_registered(client) || !self.is_registered(server) {
            return Err(DonationError::UnknownThread);
        }
        if self.calls.contains_key(&client) {
            return Err(DonationError::DuplicateCall);
        }
        self.calls.insert(client, server);
        self.refresh();
        Ok(())
    }

    /// Close a call edge after the server replies.
    pub fn end_call(&mut self, client: ThreadId, server: ThreadId) -> Result<(), DonationError> {
        match self.calls.get(&client) {
            Some(&actual) if actual == server => {
                self.calls.remove(&client);
                self.refresh();
                Ok(())
            }
            _ => Err(DonationError::StaleCall),
        }
    }

    /// Close a call edge without allocating or sorting (process death, fault and cancel context).
    ///
    /// The edge is removed immediately and the cached effective priorities of the server and its
    /// dependent closure are recomputed by a local fixed point over a fixed-capacity set
    /// ([`MAX_NO_ALLOC_AFFECTED`]). When the dependent closure exceeds that bound the graph is
    /// marked dirty and the next allocating mutation refreshes it wholesale
    /// ([`Donation::refresh_if_dirty`]); the exit path never allocates.
    pub fn end_call_no_alloc(
        &mut self,
        client: ThreadId,
        server: ThreadId,
    ) -> Result<(), DonationError> {
        match self.calls.get(&client) {
            Some(&actual) if actual == server => {
                self.calls.remove(&client);
            }
            _ => return Err(DonationError::StaleCall),
        }
        self.restore_without_alloc(server);
        Ok(())
    }

    /// Recompute the whole graph now if [`Donation::end_call_no_alloc`] overflowed its bound.
    pub fn refresh_if_dirty(&mut self) {
        if self.dirty {
            self.refresh();
        }
    }

    /// Whether an allocation-free edge removal left the cached graph stale.
    #[must_use]
    pub fn is_dirty(&self) -> bool {
        self.dirty
    }

    /// Recompute the cached priorities of `changed` and its dependent closure, without allocating.
    ///
    /// Dependents of a thread are the owners of the locks it waits on (they inherit its priority)
    /// and the servers of the calls it makes. Outside the closure the cached values are stable, so
    /// a fixed point over the closure is exact; cycles stay inside the closure by construction.
    fn restore_without_alloc(&mut self, changed: ThreadId) {
        let mut set = [0u32; MAX_NO_ALLOC_AFFECTED];
        let mut len = 1usize;
        set[0] = changed;
        let mut next = 0usize;
        let mut overflow = false;
        while next < len {
            let thread = set[next];
            next += 1;
            let mut add = |candidate: ThreadId| {
                if set[..len].contains(&candidate) {
                    return;
                }
                if len == MAX_NO_ALLOC_AFFECTED {
                    overflow = true;
                    return;
                }
                set[len] = candidate;
                len += 1;
            };
            if let Some(object) = self.turnstiles.waiting_on(thread)
                && let Some(owner) = self.turnstiles.owner(object)
            {
                add(owner);
            }
            if let Some(&server) = self.calls.get(&thread) {
                add(server);
            }
        }
        if overflow {
            self.dirty = true;
            return;
        }
        let mut sweeps = 0usize;
        loop {
            let mut any = false;
            for &thread in &set[..len] {
                let Some(base) = self.turnstiles.base_priority(thread) else {
                    continue;
                };
                let mut effective = base;
                for (&client, &server) in &self.calls {
                    if server == thread
                        && let Some(priority) = self.effective.get(&client)
                        && *priority < effective
                    {
                        effective = *priority;
                    }
                }
                let effective_ref = &self.effective;
                for object in self.turnstiles.objects() {
                    if self.turnstiles.owner(object) != Some(thread) {
                        continue;
                    }
                    self.turnstiles.for_each_waiter(object, |waiter| {
                        if let Some(priority) = effective_ref.get(&waiter)
                            && *priority < effective
                        {
                            effective = *priority;
                        }
                    });
                }
                if let Some(current) = self.effective.get_mut(&thread)
                    && *current != effective
                {
                    *current = effective;
                    any = true;
                }
            }
            if !any {
                break;
            }
            sweeps = sweeps.saturating_add(1);
            if sweeps > len {
                // Monotone decrease over a finite set: a fixed point is reached in at most one
                // sweep per thread; the bound only guards against a corrupt cached value.
                break;
            }
        }
        self.max_depth = self.max_depth.max(sweeps);
    }

    /// Cancel a client's outstanding call.
    pub fn cancel_call(&mut self, client: ThreadId) -> Result<(), DonationError> {
        if self.calls.remove(&client).is_none() {
            return Err(DonationError::StaleCall);
        }
        self.refresh();
        Ok(())
    }

    /// Remove a dead client's call edge. Returns the number of edges removed (0 or 1).
    pub fn client_death(&mut self, client: ThreadId) -> usize {
        let removed = usize::from(self.calls.remove(&client).is_some());
        if removed != 0 {
            self.refresh();
        }
        removed
    }

    /// Remove every call edge to a dead server. Returns the number of edges removed.
    pub fn server_death(&mut self, server: ThreadId) -> usize {
        let mut removed = 0usize;
        self.calls.retain(|_, &mut actual| {
            if actual == server {
                removed = removed.saturating_add(1);
                false
            } else {
                true
            }
        });
        if removed != 0 {
            self.refresh();
        }
        removed
    }

    /// Take ownership of `object` (lock operation, delegated to the turnstile).
    pub fn acquire(&mut self, object: ObjectId, thread: ThreadId) -> Result<(), Error> {
        self.turnstiles.acquire(object, thread)?;
        self.refresh();
        Ok(())
    }

    /// Block `thread` on `object` and return the owner's combined inheritance effect.
    pub fn block(&mut self, object: ObjectId, thread: ThreadId) -> Result<InheritEffect, Error> {
        let before = self.owner_effective(object);
        self.turnstiles.block(object, thread)?;
        self.refresh();
        let owner = self.turnstiles.owner(object).ok_or(Error::UnknownThread)?;
        let after = self
            .effective
            .get(&owner)
            .copied()
            .unwrap_or(Priority::LOWEST);
        Ok(InheritEffect {
            owner,
            before: before.unwrap_or(after),
            after,
        })
    }

    /// Wake the highest-priority waiter and hand it ownership (delegated).
    #[must_use]
    pub fn wake_one(&mut self, object: ObjectId) -> Option<ThreadId> {
        let woken = self.turnstiles.wake_one(object);
        self.refresh();
        woken
    }

    /// Release and wake every waiter (delegated).
    #[must_use]
    pub fn wake_all(&mut self, object: ObjectId) -> Vec<ThreadId> {
        let woken = self.turnstiles.wake_all(object);
        self.refresh();
        woken
    }

    /// Cancel a waiter (delegated).
    pub fn cancel(&mut self, object: ObjectId, thread: ThreadId) -> CancelOutcome {
        let outcome = self.turnstiles.cancel(object, thread);
        self.refresh();
        outcome
    }

    /// Recompute the owner's inheritance effect (delegated).
    pub fn boost(&mut self, object: ObjectId) -> Result<Option<InheritEffect>, Error> {
        let effect = self.turnstiles.boost(object)?;
        self.refresh();
        Ok(effect)
    }

    /// Drop the inheritance contribution (delegated).
    pub fn unboost(&mut self, object: ObjectId) -> Result<Option<InheritEffect>, Error> {
        let effect = self.turnstiles.unboost(object)?;
        self.refresh();
        Ok(effect)
    }

    /// The owner of `object` (delegated).
    #[must_use]
    pub fn owner(&self, object: ObjectId) -> Option<ThreadId> {
        self.turnstiles.owner(object)
    }

    /// The waiters of `object` (delegated).
    #[must_use]
    pub fn waiters(&self, object: ObjectId) -> Vec<ThreadId> {
        self.turnstiles.waiters(object)
    }

    /// The number of waiters on `object` (delegated).
    #[must_use]
    pub fn waiter_count(&self, object: ObjectId) -> usize {
        self.turnstiles.waiter_count(object)
    }

    /// The object `thread` waits on (delegated).
    #[must_use]
    pub fn waiting_on(&self, thread: ThreadId) -> Option<ObjectId> {
        self.turnstiles.waiting_on(thread)
    }

    /// The objects owned by `thread` (delegated).
    #[must_use]
    pub fn owned_objects(&self, thread: ThreadId) -> Vec<ObjectId> {
        self.turnstiles.owned_objects(thread)
    }

    /// The outstanding call edges, in `(client, server)` order.
    #[must_use]
    pub fn call_edges(&self) -> Vec<(ThreadId, ThreadId)> {
        self.calls
            .iter()
            .map(|(&client, &server)| (client, server))
            .collect()
    }

    /// Number of threads currently registered in the donation graph.
    ///
    /// Diagnostic counter for the thread-exit lifetime: it must stay flat across SSH sessions.
    #[must_use]
    pub fn registered_threads(&self) -> usize {
        self.turnstiles.threads().count()
    }

    /// The maximum donation-chain depth observed so far (an upper bound on the longest chain).
    #[must_use]
    pub fn max_chain_depth(&self) -> usize {
        self.max_depth
    }

    /// Check every donation invariant.
    ///
    /// Returns the first violation. Never panics; reads only the current state.
    pub fn check_invariants(&self) -> Result<(), DonationViolation> {
        for (&client, &server) in &self.calls {
            if !self.is_registered(client) || !self.is_registered(server) {
                return Err(DonationViolation::DanglingCall);
            }
        }
        let (recomputed, _) = self.compute_effective();
        if recomputed != self.effective {
            return Err(DonationViolation::EffectiveMismatch);
        }
        for (thread, base) in self.turnstiles.threads() {
            let has_donor = self.calls.values().any(|&server| server == thread)
                || self
                    .turnstiles
                    .owned_objects(thread)
                    .iter()
                    .any(|&object| self.turnstiles.waiter_count(object) > 0);
            if !has_donor && self.effective.get(&thread).copied() != Some(base) {
                return Err(DonationViolation::BaseNotRestored);
            }
        }
        Ok(())
    }

    fn is_registered(&self, thread: ThreadId) -> bool {
        self.turnstiles.base_priority(thread).is_some()
    }

    fn owner_effective(&self, object: ObjectId) -> Option<Priority> {
        let owner = self.turnstiles.owner(object)?;
        self.effective.get(&owner).copied()
    }

    /// Compute the least fixed point over lock edges and call edges.
    fn compute_effective(&self) -> (BTreeMap<ThreadId, Priority>, usize) {
        let mut effective: BTreeMap<ThreadId, Priority> = self.turnstiles.threads().collect();
        let mut depth = 0usize;
        loop {
            let mut changed = false;
            for object in self.turnstiles.objects() {
                let Some(owner) = self.turnstiles.owner(object) else {
                    continue;
                };
                for waiter in self.turnstiles.waiters(object) {
                    let Some(priority) = effective.get(&waiter).copied() else {
                        continue;
                    };
                    if let Some(current) = effective.get_mut(&owner)
                        && priority < *current
                    {
                        *current = priority;
                        changed = true;
                    }
                }
            }
            for (&client, &server) in &self.calls {
                let Some(priority) = effective.get(&client).copied() else {
                    continue;
                };
                if let Some(current) = effective.get_mut(&server)
                    && priority < *current
                {
                    *current = priority;
                    changed = true;
                }
            }
            if !changed {
                return (effective, depth);
            }
            depth = depth.saturating_add(1);
        }
    }

    fn refresh(&mut self) {
        let (effective, depth) = self.compute_effective();
        self.effective = effective;
        self.max_depth = self.max_depth.max(depth);
        self.dirty = false;
    }

    /// Recompute the combined fixed point in place, without cloning either map.
    ///
    /// The thread-exit path cannot allocate; every registered thread has an `effective` entry, so
    /// resetting to the base priorities and iterating over the current edges is exact.
    fn refresh_without_alloc(&mut self) {
        for (thread, base) in self.turnstiles.threads() {
            if let Some(effective) = self.effective.get_mut(&thread) {
                *effective = base;
            }
        }
        let mut sweeps = 0usize;
        loop {
            let mut changed = false;
            for object in self.turnstiles.objects() {
                let Some(owner) = self.turnstiles.owner(object) else {
                    continue;
                };
                let mut pi = Priority::LOWEST;
                let effective = &self.effective;
                self.turnstiles.for_each_waiter(object, |waiter| {
                    if let Some(priority) = effective.get(&waiter).copied() {
                        pi = pi.min(priority);
                    }
                });
                if let Some(current) = self.effective.get_mut(&owner)
                    && pi < *current
                {
                    *current = pi;
                    changed = true;
                }
            }
            for (&client, &server) in &self.calls {
                if let Some(priority) = self.effective.get(&client).copied()
                    && let Some(current) = self.effective.get_mut(&server)
                    && priority < *current
                {
                    *current = priority;
                    changed = true;
                }
            }
            if !changed {
                break;
            }
            sweeps = sweeps.saturating_add(1);
        }
        self.max_depth = self.max_depth.max(sweeps);
        self.dirty = false;
    }
}

impl Default for Donation {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(feature = "lockstat")]
impl Donation {
    /// Snapshot the lock statistics for `object` (delegated to the turnstile).
    #[must_use]
    pub fn stats(&self, object: ObjectId) -> super::LockStats {
        self.turnstiles.stats(object)
    }

    /// The live turnstile object ids (delegated to the turnstile).
    pub fn objects(&self) -> impl Iterator<Item = ObjectId> + '_ {
        self.turnstiles.objects()
    }

    /// The object ids with recorded statistics; they outlive a reclaimed turnstile.
    pub fn stats_objects(&self) -> impl Iterator<Item = ObjectId> + '_ {
        self.turnstiles.stats_objects()
    }
}

#[cfg(test)]
#[path = "donation_ut.rs"]
mod tests;
