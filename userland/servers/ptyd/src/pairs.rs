// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Named pty pairs.
//!
//! Pair `0` keeps the historical `pty:master` / `pty:slave` path. Higher ids let independent
//! terminal sessions coexist, which `talk` uses for a second local peer. Each pair owns its own
//! line discipline, queues and foreground group.

use alloc::collections::BTreeMap;

use crate::{Pty, SignalSink};

/// The stable inode of one pty endpoint: pair `N` master is `2N + 1`, slave is `2N + 2`.
///
/// The donor devfs pty nodes carry a device inode; Raven's scheme `Stat` record has `ino` only,
/// so the pair and role provide the identity that `fstat` reports. Both values are non-zero and
/// a master never shares an inode with a slave.
#[must_use]
pub const fn inode(pair: u64, slave: bool) -> u64 {
    pair.wrapping_mul(2).wrapping_add(if slave { 2 } else { 1 })
}

/// The pty pair table.
pub struct Pairs<S: SignalSink + Default> {
    entries: BTreeMap<u64, Pty<S>>,
}

impl<S: SignalSink + Default> Default for Pairs<S> {
    fn default() -> Self {
        Self::new()
    }
}

impl<S: SignalSink + Default> Pairs<S> {
    /// An empty pair table.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            entries: BTreeMap::new(),
        }
    }

    /// Get or create pair `id`.
    pub fn open(&mut self, id: u64) -> &mut Pty<S> {
        self.entries
            .entry(id)
            .or_insert_with(|| Pty::new(S::default()))
    }

    /// Get an existing pair without creating it.
    pub fn get_mut(&mut self, id: u64) -> Option<&mut Pty<S>> {
        self.entries.get_mut(&id)
    }

    /// Allocate the lowest free pair id.
    ///
    /// This is the clone-device rule the donors use: the pty server owns the unit pool, so two
    /// processes can never open the same pair by accident. Pair `0` stays reserved for the
    /// historical named `pty:master`/`pty:slave` paths and is skipped here.
    pub fn alloc(&mut self) -> u64 {
        let mut id = 1u64;
        while self.entries.contains_key(&id) {
            id = id.wrapping_add(1);
        }
        id
    }

    /// Remove a pair once its last scheme file has closed.
    pub fn remove(&mut self, id: u64) -> bool {
        self.entries.remove(&id).is_some()
    }

    /// Number of live pairs.
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// True when no pair exists.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

#[cfg(test)]
#[path = "pairs_ut.rs"]
mod tests;
