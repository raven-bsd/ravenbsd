// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Logged-in terminal peer registry.
//!
//! Each record names a user, the tty path the user is attached to, the owning ttyd session and
//! whether the terminal currently accepts messages. The table is pure and host-testable; the
//! server owns the live handles and a `Peer` disappears when its handle closes or its session is
//! removed.

use alloc::collections::BTreeMap;
use alloc::string::{String, ToString};
use alloc::vec::Vec;

/// One logged-in terminal.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Peer {
    /// User name.
    pub user: String,
    /// Tty path, for example `pty:1/slave` or `tty:console`.
    pub tty: String,
    /// Owning ttyd session id.
    pub session: u32,
    /// Whether the terminal currently accepts messages.
    pub writable: bool,
}

/// The peer registry.
pub struct Peers {
    entries: BTreeMap<u64, Peer>,
    next: u64,
}

impl Default for Peers {
    fn default() -> Self {
        Self::new()
    }
}

impl Peers {
    /// An empty registry.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            entries: BTreeMap::new(),
            next: 1,
        }
    }

    /// Register one terminal and return its peer id.
    ///
    /// A previous record for the same user and tty path is replaced, so a re-login does not leave
    /// a stale terminal behind.
    pub fn register(&mut self, user: &str, tty: &str, session: u32, writable: bool) -> u64 {
        let stale: Vec<u64> = self
            .entries
            .iter()
            .filter(|(_, peer)| peer.user == user && peer.tty == tty)
            .map(|(id, _)| *id)
            .collect();
        for id in stale {
            self.entries.remove(&id);
        }
        let id = self.next;
        self.next = self.next.wrapping_add(1).max(1);
        self.entries.insert(
            id,
            Peer {
                user: user.to_string(),
                tty: tty.to_string(),
                session,
                writable,
            },
        );
        id
    }

    /// Remove one peer id.
    pub fn unregister(&mut self, id: u64) -> bool {
        self.entries.remove(&id).is_some()
    }

    /// Remove every terminal owned by `session`; returns the number removed.
    pub fn unregister_session(&mut self, session: u32) -> usize {
        let stale: Vec<u64> = self
            .entries
            .iter()
            .filter(|(_, peer)| peer.session == session)
            .map(|(id, _)| *id)
            .collect();
        let count = stale.len();
        for id in stale {
            self.entries.remove(&id);
        }
        count
    }

    /// The record for one peer id.
    #[must_use]
    pub fn get(&self, id: u64) -> Option<&Peer> {
        self.entries.get(&id)
    }

    /// Find a user's terminal, optionally restricted to one tty path.
    ///
    /// The oldest matching record wins, matching the historical `who` lookup closest first.
    #[must_use]
    pub fn lookup(&self, user: &str, terminal: Option<&str>) -> Option<&Peer> {
        self.entries
            .values()
            .find(|peer| peer.user == user && terminal.is_none_or(|terminal| peer.tty == terminal))
    }

    /// Set one record's writable state.
    pub fn set_writable(&mut self, id: u64, writable: bool) -> bool {
        match self.entries.get_mut(&id) {
            Some(peer) => {
                peer.writable = writable;
                true
            }
            None => false,
        }
    }

    /// Update every terminal owned by `session`; returns the number updated.
    pub fn set_writable_for_session(&mut self, session: u32, writable: bool) -> usize {
        let mut count = 0usize;
        for peer in self.entries.values_mut() {
            if peer.session == session {
                peer.writable = writable;
                count = count.saturating_add(1);
            }
        }
        count
    }

    /// Number of registered terminals.
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// True when no terminal is registered.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

#[cfg(test)]
#[path = "peers_ut.rs"]
mod tests;
