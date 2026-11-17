// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Session and foreground process-group records.
//!
//! The terminal server cannot yet change the kernel controlling terminal; it records the session
//! id, foreground group and hangup state that the tty and procd glue will use. The table is pure
//! and host-testable.

use alloc::collections::BTreeMap;

/// One terminal session.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Session {
    /// Foreground process group.
    pub fg_pgrp: i32,
    /// Session id.
    pub sid: i32,
    /// True after a hangup.
    pub hung: bool,
    /// True when terminal writes are allowed.
    pub write_allowed: bool,
}

impl Default for Session {
    fn default() -> Self {
        Self {
            fg_pgrp: 0,
            sid: 0,
            hung: false,
            write_allowed: true,
        }
    }
}

/// The session table.
pub struct Sessions {
    entries: BTreeMap<u32, Session>,
    next: u32,
}

impl Default for Sessions {
    fn default() -> Self {
        Self::new()
    }
}

impl Sessions {
    /// An empty session table.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            entries: BTreeMap::new(),
            next: 1,
        }
    }

    /// Allocate one session and return its id.
    pub fn open(&mut self) -> u32 {
        let id = self.next;
        self.next = self.next.wrapping_add(1).max(1);
        self.entries.insert(id, Session::default());
        id
    }

    /// The session record, if it exists.
    #[must_use]
    pub fn get(&self, id: u32) -> Option<&Session> {
        self.entries.get(&id)
    }

    /// Set the foreground process group.
    pub fn set_fg(&mut self, id: u32, pgrp: i32) -> bool {
        match self.entries.get_mut(&id) {
            Some(session) => {
                session.fg_pgrp = pgrp;
                true
            }
            None => false,
        }
    }

    /// The foreground process group, or zero when the session is unknown.
    #[must_use]
    pub fn fg(&self, id: u32) -> i32 {
        self.entries.get(&id).map_or(0, |session| session.fg_pgrp)
    }

    /// Set the session id.
    pub fn set_sid(&mut self, id: u32, sid: i32) -> bool {
        match self.entries.get_mut(&id) {
            Some(session) => {
                session.sid = sid;
                true
            }
            None => false,
        }
    }

    /// The session id, or zero when the session is unknown.
    #[must_use]
    pub fn sid(&self, id: u32) -> i32 {
        self.entries.get(&id).map_or(0, |session| session.sid)
    }

    /// Mark a session hung up.
    pub fn hangup(&mut self, id: u32) -> bool {
        match self.entries.get_mut(&id) {
            Some(session) => {
                session.hung = true;
                true
            }
            None => false,
        }
    }

    /// True when the session was hung up.
    #[must_use]
    pub fn hung(&self, id: u32) -> bool {
        self.entries.get(&id).is_some_and(|session| session.hung)
    }

    /// True when terminal writes are allowed for the session.
    #[must_use]
    pub fn write_allowed(&self, id: u32) -> bool {
        self.entries
            .get(&id)
            .is_some_and(|session| session.write_allowed)
    }

    /// Set the session's terminal write permission.
    pub fn set_write_allowed(&mut self, id: u32, allowed: bool) -> bool {
        match self.entries.get_mut(&id) {
            Some(session) => {
                session.write_allowed = allowed;
                true
            }
            None => false,
        }
    }

    /// Remove a session.
    pub fn close(&mut self, id: u32) -> bool {
        self.entries.remove(&id).is_some()
    }

    /// Number of live sessions.
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// True when no session exists.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

#[cfg(test)]
#[path = "sessions_ut.rs"]
mod tests;
