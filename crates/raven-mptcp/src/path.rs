// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Transport-independent MPTCP path manager.
//!
//! The manager stores the Address-ID-to-address mapping that ADD_ADDR, REMOVE_ADDR and MP_PRIO
//! operate on. It has no socket, no timer and no transport trait; a caller decides when to send
//! a signal and what a subflow is. The table is bounded ([`MAX_PATHS`]) and duplicate Address IDs
//! are rejected so a malicious ADD_ADDR cannot grow it without bound.

use alloc::vec::Vec;

use crate::Error;
use crate::option::Address;

/// Maximum number of announced paths per connection.
pub const MAX_PATHS: usize = 8;

/// One path known to the connection.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Path {
    /// Address ID from ADD_ADDR or MP_JOIN (0..=255).
    pub address_id: u8,
    /// Advertised address.
    pub address: Address,
    /// Advertised port; `None` means the subflow's current port (RFC 8684 section 3.4.1).
    pub port: Option<u16>,
    /// MP_PRIO/MP_JOIN backup flag: use only when no regular path is available.
    pub backup: bool,
    /// Optional local subflow handle; opaque to this crate.
    pub subflow: Option<u32>,
}

impl Path {
    /// Create a path entry.
    #[must_use]
    pub fn new(address_id: u8, address: Address, port: Option<u16>) -> Self {
        Self {
            address_id,
            address,
            port,
            backup: false,
            subflow: None,
        }
    }
}

/// A bounded set of announced paths.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PathManager {
    paths: Vec<Path>,
}

impl PathManager {
    /// An empty manager.
    #[must_use]
    pub fn new() -> Self {
        Self { paths: Vec::new() }
    }

    /// Number of known paths.
    #[must_use]
    pub fn len(&self) -> usize {
        self.paths.len()
    }

    /// Whether the table is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.paths.is_empty()
    }

    /// All paths in announcement order.
    #[must_use]
    pub fn paths(&self) -> &[Path] {
        &self.paths
    }

    /// Find a path by Address ID.
    #[must_use]
    pub fn get(&self, address_id: u8) -> Option<&Path> {
        self.paths.iter().find(|path| path.address_id == address_id)
    }

    /// Announce a path; duplicate IDs and a full table are errors.
    pub fn announce(&mut self, path: Path) -> Result<(), Error> {
        if self.get(path.address_id).is_some() {
            return Err(Error::Duplicate);
        }
        if self.paths.len() >= MAX_PATHS {
            return Err(Error::PathTableFull);
        }
        self.paths.push(path);
        Ok(())
    }

    /// Remove one path by Address ID.
    pub fn remove(&mut self, address_id: u8) -> Result<Path, Error> {
        let index = self
            .paths
            .iter()
            .position(|path| path.address_id == address_id)
            .ok_or(Error::NotFound)?;
        Ok(self.paths.remove(index))
    }

    /// Remove every listed Address ID; unknown IDs are ignored, as RFC 8684 section 3.4.2 says.
    ///
    /// Returns the number removed.
    pub fn remove_ids(&mut self, ids: &[u8]) -> usize {
        let before = self.paths.len();
        self.paths.retain(|path| !ids.contains(&path.address_id));
        before - self.paths.len()
    }

    /// Set or clear the backup flag of one path (MP_PRIO).
    pub fn set_backup(&mut self, address_id: u8, backup: bool) -> Result<(), Error> {
        let path = self
            .paths
            .iter_mut()
            .find(|path| path.address_id == address_id)
            .ok_or(Error::NotFound)?;
        path.backup = backup;
        Ok(())
    }

    /// Attach an opaque local subflow handle to a path.
    pub fn set_subflow(&mut self, address_id: u8, subflow: Option<u32>) -> Result<(), Error> {
        let path = self
            .paths
            .iter_mut()
            .find(|path| path.address_id == address_id)
            .ok_or(Error::NotFound)?;
        path.subflow = subflow;
        Ok(())
    }

    /// Select the preferred path: the lowest-ID regular path, or the lowest-ID backup path when
    /// every path is a backup. RFC 8684 section 3.3.8: backup paths are used only when no
    /// regular path is available.
    #[must_use]
    pub fn select(&self) -> Option<&Path> {
        self.paths
            .iter()
            .filter(|path| !path.backup)
            .min_by_key(|path| path.address_id)
            .or_else(|| self.paths.iter().min_by_key(|path| path.address_id))
    }

    /// The number of regular (non-backup) paths.
    #[must_use]
    pub fn regular_count(&self) -> usize {
        self.paths.iter().filter(|path| !path.backup).count()
    }
}

#[cfg(test)]
#[path = "path_ut.rs"]
mod tests;
