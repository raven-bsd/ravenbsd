// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The `unveil` path table.

use alloc::vec::Vec;

use crate::errors::{E2BIG, EACCES, EINVAL, ENOENT, EPERM, Errno};
use crate::limits::MAX_UNVEIL;

/// The permissions of one unveil entry.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct UnveilPerms(pub u8);

impl UnveilPerms {
    /// Read.
    pub const R: u8 = 1;
    /// Write.
    pub const W: u8 = 2;
    /// Execute.
    pub const X: u8 = 4;
    /// Create.
    pub const C: u8 = 8;

    /// Whether every bit of `other` is present.
    #[must_use]
    pub const fn contains(self, other: Self) -> bool {
        self.0 & other.0 == other.0
    }

    /// The union.
    #[must_use]
    pub const fn union(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }

    /// Whether no permission is set.
    #[must_use]
    pub const fn is_empty(self) -> bool {
        self.0 == 0
    }
}

/// The table state.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UnveilState {
    /// No unveil call has been made; everything is visible.
    Unset,
    /// The table is active.
    Active,
    /// The table is locked.
    Locked,
}

/// The per-process unveil table.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UnveilTable {
    entries: Vec<(Vec<u8>, UnveilPerms)>,
    state: UnveilState,
}

impl UnveilTable {
    /// An unset table.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            entries: Vec::new(),
            state: UnveilState::Unset,
        }
    }

    /// The current state.
    #[must_use]
    pub const fn state(&self) -> UnveilState {
        self.state
    }

    /// Number of entries.
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether no entry is stored.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Apply one `unveil` call.
    pub fn unveil(&mut self, path: Option<&[u8]>, perms: Option<&[u8]>) -> Result<(), Errno> {
        if self.state == UnveilState::Locked {
            return Err(EPERM);
        }
        let (path, perms) = match (path, perms) {
            (None, None) => {
                self.state = UnveilState::Locked;
                return Ok(());
            }
            (Some(path), Some(perms)) => (path, perms),
            _ => return Err(EINVAL),
        };
        if !path.starts_with(b"/") {
            return Err(EINVAL);
        }
        let perms = parse_perms(perms)?;
        if let Some(entry) = self
            .entries
            .iter_mut()
            .find(|(existing, _)| existing.as_slice() == path)
        {
            entry.1 = perms;
        } else {
            if self.entries.len() >= MAX_UNVEIL {
                return Err(E2BIG);
            }
            self.entries.push((path.to_vec(), perms));
        }
        if self.state == UnveilState::Unset {
            self.state = UnveilState::Active;
        }
        Ok(())
    }

    /// Check access to `path` for `need`.
    pub fn check(&self, path: &[u8], need: UnveilPerms) -> Result<(), Errno> {
        if self.state == UnveilState::Unset {
            return Ok(());
        }
        let mut best: Option<&(Vec<u8>, UnveilPerms)> = None;
        for entry in &self.entries {
            if covers(&entry.0, path) {
                let replace = best.is_none_or(|current| entry.0.len() > current.0.len());
                if replace {
                    best = Some(entry);
                }
            }
        }
        let Some((_, perms)) = best else {
            return Err(ENOENT);
        };
        if perms.contains(need) {
            Ok(())
        } else {
            Err(EACCES)
        }
    }

    /// Return to the unset state.
    pub fn clear(&mut self) {
        self.entries.clear();
        self.state = UnveilState::Unset;
    }
}

impl Default for UnveilTable {
    fn default() -> Self {
        Self::new()
    }
}

fn parse_perms(text: &[u8]) -> Result<UnveilPerms, Errno> {
    let mut bits = 0u8;
    for byte in text {
        bits |= match *byte {
            b'r' => UnveilPerms::R,
            b'w' => UnveilPerms::W,
            b'x' => UnveilPerms::X,
            b'c' => UnveilPerms::C,
            _ => return Err(EINVAL),
        };
    }
    Ok(UnveilPerms(bits))
}

fn covers(entry: &[u8], path: &[u8]) -> bool {
    if entry == path {
        return true;
    }
    if path.len() <= entry.len() {
        return false;
    }
    path.get(..entry.len()) == Some(entry) && path.get(entry.len()) == Some(&b'/')
}

#[cfg(test)]
#[path = "unveil_ut.rs"]
mod tests;
