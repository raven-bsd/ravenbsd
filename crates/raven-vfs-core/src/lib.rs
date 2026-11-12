// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Pure mount table and path routing for `vfsd`.
//!
//! The table matches whole path components only: `/tmp` matches `/tmp/a` and
//! `/tmp` but not `/tmpx`.  Nested mounts are allowed; the longest matching
//! mount wins.  The server receives a path relative to its mount root (the root
//! mount maps `/a` to `a`).
//!
//! This crate is `no_std` + `alloc`, never allocates while resolving a path and
//! never panics on client input.

#![cfg_attr(not(test), no_std)]
#![forbid(unsafe_code)]
#![warn(missing_docs)]

extern crate alloc;

use alloc::collections::{BTreeMap, BTreeSet};
use alloc::string::{String, ToString};
use alloc::vec::Vec;

use raven_abi::errno::Errno;
use raven_proto_fs::{MAX_PATH, normalize_absolute};

/// Maximum number of simultaneous mounts.
pub const MAX_MOUNTS: usize = 32;

/// Default mount table written to `/etc/mounts`.
pub const DEFAULT_MOUNTS: &[(&str, &str)] = &[
    ("/", "initfs:"),
    ("/tmp", "ramfs:"),
    ("/dev/null", "null:"),
    ("/dev/zero", "zero:"),
    ("/dev/random", "rand:"),
];

/// One mounted scheme.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Mount {
    /// Stable mount id (never reused within one table).
    pub id: u32,
    /// Absolute, normalized mount point.
    pub path: String,
    /// Scheme name including the trailing colon.
    pub scheme: String,
}

/// The mount table.
#[derive(Clone, Debug)]
pub struct MountTable {
    mounts: BTreeMap<u32, Mount>,
    next_id: u32,
}

impl Default for MountTable {
    fn default() -> Self {
        Self::new()
    }
}

impl MountTable {
    /// Create an empty table.
    pub const fn new() -> Self {
        Self {
            mounts: BTreeMap::new(),
            next_id: 1,
        }
    }

    /// Mount `scheme` at `path` and return the stable mount id.
    ///
    /// `path` must be absolute and already normalized.  A duplicate mount point
    /// is `EEXIST`; a full table is `ENOSPC`; a scheme without a non-empty name
    /// and a trailing colon is `EINVAL`.
    pub fn mount(&mut self, path: &str, scheme: &str) -> Result<u32, Errno> {
        if normalized(path)? != path || !valid_scheme(scheme) {
            return Err(Errno::EINVAL);
        }
        if self.mounts.values().any(|mount| mount.path == path) {
            return Err(Errno::EEXIST);
        }
        if self.mounts.len() >= MAX_MOUNTS {
            return Err(Errno::ENOSPC);
        }
        let id = self.next_id;
        self.next_id = self.next_id.checked_add(1).ok_or(Errno::ENFILE)?;
        self.mounts.insert(
            id,
            Mount {
                id,
                path: path.to_string(),
                scheme: scheme.to_string(),
            },
        );
        Ok(id)
    }

    /// Mount the compiled-in defaults; returns the number mounted.
    pub fn mount_defaults(&mut self) -> Result<usize, Errno> {
        for (path, scheme) in DEFAULT_MOUNTS {
            self.mount(path, scheme)?;
        }
        Ok(DEFAULT_MOUNTS.len())
    }

    /// Unmount `path`.
    ///
    /// `open_files` is the number of files the server knows are open below the
    /// mount; any nonzero value is `EBUSY`.  `/` cannot be removed while another
    /// mount exists.  An unknown mount point is `ENOENT`.
    pub fn umount(&mut self, path: &str, open_files: usize) -> Result<(), Errno> {
        if normalized(path)? != path {
            return Err(Errno::EINVAL);
        }
        let id = self
            .mounts
            .iter()
            .find_map(|(id, mount)| (mount.path == path).then_some(*id))
            .ok_or(Errno::ENOENT)?;
        if path == "/" && self.mounts.len() > 1 {
            return Err(Errno::EBUSY);
        }
        if open_files != 0 {
            return Err(Errno::EBUSY);
        }
        self.mounts.remove(&id);
        Ok(())
    }

    /// Resolve an absolute, normalized `path` to `(mount_id, path_in_mount)`.
    ///
    /// The mount with the longest whole-component prefix wins.  The returned
    /// suffix has no leading slash; an exact mount point maps to `""`.
    pub fn resolve<'a>(&self, path: &'a str) -> Result<(u32, &'a str), Errno> {
        if !path.starts_with('/') {
            return Err(Errno::EINVAL);
        }
        let mut best: Option<(&Mount, &'a str)> = None;
        for mount in self.mounts.values() {
            let suffix = if mount.path == "/" {
                path.get(1..)
            } else if path == mount.path {
                Some("")
            } else if let Some(rest) = path.strip_prefix(mount.path.as_str())
                && rest.starts_with('/')
            {
                rest.get(1..)
            } else {
                None
            };
            let Some(suffix) = suffix else {
                continue;
            };
            if best.is_none_or(|(current, _)| current.path.len() < mount.path.len()) {
                best = Some((mount, suffix));
            }
        }
        best.map(|(mount, suffix)| (mount.id, suffix))
            .ok_or(Errno::ENOENT)
    }

    /// Look up a mount by id.
    pub fn get(&self, id: u32) -> Option<&Mount> {
        self.mounts.get(&id)
    }

    /// Every mount, in id order.
    pub fn mounts(&self) -> impl Iterator<Item = &Mount> {
        self.mounts.values()
    }

    /// Direct child mounts of `path`, sorted by mount point.
    ///
    /// `/` lists `/tmp` but not `/dev/null`; `/tmp` lists `/tmp/x`.
    pub fn mounts_below(&self, path: &str) -> Vec<&Mount> {
        let mut result = Vec::new();
        for mount in self.mounts.values() {
            if mount.path == path {
                continue;
            }
            let relative = if path == "/" {
                mount.path.strip_prefix('/')
            } else {
                mount
                    .path
                    .strip_prefix(path)
                    .and_then(|rest| rest.strip_prefix('/'))
            };
            if relative.is_some_and(|rest| !rest.is_empty() && !rest.contains('/')) {
                result.push(mount);
            }
        }
        result.sort_by(|left, right| left.path.cmp(&right.path));
        result
    }

    /// Return `EXDEV` when a rename crosses mounts, otherwise `Ok(())`.
    pub fn rename(&self, from: &str, to: &str) -> Result<(), Errno> {
        let (from_mount, _) = self.resolve(from)?;
        let (to_mount, _) = self.resolve(to)?;
        if from_mount == to_mount {
            Ok(())
        } else {
            Err(Errno::EXDEV)
        }
    }

    /// Number of mounts.
    pub fn len(&self) -> usize {
        self.mounts.len()
    }

    /// Whether the table is empty.
    pub fn is_empty(&self) -> bool {
        self.mounts.is_empty()
    }
}

/// Normalize an absolute path; the result must equal the input for a mount
/// point or a call that requires a pre-normalized path.
pub fn normalized(path: &str) -> Result<String, Errno> {
    let mut buffer = [0u8; MAX_PATH];
    normalize_absolute(path, &mut buffer)
        .map(|text| text.to_string())
        .map_err(|error| error.errno())
}

/// Whether `scheme` is a non-empty name followed by exactly one trailing colon.
pub fn valid_scheme(scheme: &str) -> bool {
    let Some(name) = scheme.strip_suffix(':') else {
        return false;
    };
    !name.is_empty()
        && name.len() <= 63
        && !name.contains('/')
        && !name.contains('\0')
        && !name.contains(':')
}

/// Parse `/etc/mounts` contents: one `<absolute-path> <scheme>` pair per line.
///
/// Blank lines and lines whose first non-space character is `#` are ignored.
/// Each path must already be normalized; each scheme must be valid.  Duplicate
/// mount points are `EEXIST`.
pub fn parse_mounts(text: &str) -> Result<Vec<(String, String)>, Errno> {
    let mut result = Vec::new();
    let mut seen = BTreeSet::new();
    for line in text.lines() {
        let line = line.split('#').next().unwrap_or("").trim();
        if line.is_empty() {
            continue;
        }
        let mut fields = line.split_whitespace();
        let path = fields.next().ok_or(Errno::EINVAL)?;
        let scheme = fields.next().ok_or(Errno::EINVAL)?;
        if fields.next().is_some() || normalized(path)? != path || !valid_scheme(scheme) {
            return Err(Errno::EINVAL);
        }
        if !seen.insert(path.to_string()) {
            return Err(Errno::EEXIST);
        }
        result.push((path.to_string(), scheme.to_string()));
    }
    Ok(result)
}

#[cfg(test)]
#[path = "lib_ut.rs"]
mod tests;
