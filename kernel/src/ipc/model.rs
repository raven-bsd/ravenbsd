// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Pure scheme-IPC policy: names, namespaces, open flags and rights.
//!
//! This file is compiled both into the kernel and into the host-testable `raven-ipc` crate. It
//! has no kernel dependencies and no unsafe code.

use alloc::string::{String, ToString};
use alloc::vec::Vec;

use raven_abi::types::Rights;

/// Open for reading.
pub const OPEN_READ: u32 = 1;
/// Open for writing.
pub const OPEN_WRITE: u32 = 2;
/// Create the file if the server supports it.
pub const OPEN_CREATE: u32 = 4;
/// All open flags understood in S11.
pub const OPEN_MASK: u32 = OPEN_READ | OPEN_WRITE | OPEN_CREATE;

/// Maximum scheme name length.
pub const NAME_MAX: usize = 64;
/// Maximum path suffix length (matches the packet field).
pub const PATH_MAX: usize = 256;

/// Why an open path was rejected.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PathError {
    /// The path is empty or longer than [`PATH_MAX`].
    BadLength,
    /// The path has no `scheme:` separator.
    MissingScheme,
    /// The scheme name contains a forbidden byte or is empty.
    BadScheme,
}

/// True when `name` matches `[a-z0-9:_-]{1,64}`.
pub fn valid_scheme_name(name: &str) -> bool {
    let bytes = name.as_bytes();
    if bytes.is_empty() || bytes.len() > NAME_MAX {
        return false;
    }
    bytes.iter().all(|byte| {
        matches!(
            *byte,
            b'a'..=b'z' | b'0'..=b'9' | b':' | b'_' | b'-'
        )
    })
}

/// True when `path` is a valid scheme path (`scheme:rest`).
pub fn split_path(path: &str) -> Result<(&str, &str), PathError> {
    if path.is_empty() || path.len() > PATH_MAX {
        return Err(PathError::BadLength);
    }
    let Some(colon) = path.find(':') else {
        return Err(PathError::MissingScheme);
    };
    let (scheme, rest) = path.split_at(colon);
    if !valid_scheme_name(scheme) {
        return Err(PathError::BadScheme);
    }
    let rest = rest.get(1..).unwrap_or("");
    Ok((scheme, rest))
}

/// A process's visible schemes.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Namespace {
    /// The root namespace sees every registered scheme.
    pub wildcard: bool,
    /// Explicitly allowed scheme names.
    pub names: Vec<String>,
    /// Allowed scheme-name prefixes.
    pub prefixes: Vec<String>,
}

impl Namespace {
    /// The root namespace (the first process and kernel process).
    pub fn root() -> Self {
        Self {
            wildcard: true,
            names: Vec::new(),
            prefixes: Vec::new(),
        }
    }

    /// A child snapshot. A wildcard parent snapshots the currently registered names; an explicit
    /// parent copies its list. The child never inherits the wildcard.
    pub fn child_snapshot(&self, registered: &[String]) -> Self {
        if self.wildcard {
            Self {
                wildcard: false,
                names: registered.to_vec(),
                prefixes: Vec::new(),
            }
        } else {
            Self {
                wildcard: false,
                names: self.names.clone(),
                prefixes: self.prefixes.clone(),
            }
        }
    }

    /// True when `scheme` is visible.
    pub fn allows(&self, scheme: &str) -> bool {
        self.wildcard
            || self.names.iter().any(|name| name == scheme)
            || self
                .prefixes
                .iter()
                .any(|prefix| !prefix.is_empty() && scheme.starts_with(prefix))
    }

    /// Allow every scheme name beginning with `prefix`. Adding an existing prefix is a no-op.
    pub fn allow_prefix(&mut self, prefix: &str) {
        if !prefix.is_empty() && !self.prefixes.iter().any(|item| item == prefix) {
            self.prefixes.push(prefix.to_string());
        }
    }

    /// True when this namespace may grant `prefix` to itself or to a child.
    ///
    /// A wildcard (root) namespace may grant any non-empty prefix. An explicit namespace may only
    /// grant a prefix that narrows one of its own prefixes: granting `net-tcp` under `net` is a
    /// subset, while granting `net` when only the exact name `net` is visible would widen the
    /// child's authority and is refused.
    pub fn can_grant_prefix(&self, prefix: &str) -> bool {
        if prefix.is_empty() {
            return false;
        }
        self.wildcard
            || self
                .prefixes
                .iter()
                .any(|existing| prefix.starts_with(existing))
    }

    /// Remove `prefix`; removing an absent prefix is a no-op. Exact names stay.
    pub fn remove_prefix(&mut self, prefix: &str) {
        self.prefixes.retain(|item| item != prefix);
    }

    /// Remove all prefixes; exact names stay.
    pub fn clear_prefixes(&mut self) {
        self.prefixes.clear();
    }

    /// Add a scheme name if it is not already visible.
    pub fn add(&mut self, scheme: &str) {
        if !self.allows(scheme) {
            self.names.push(scheme.to_string());
        }
    }
}

/// Rights granted by `Open(flags)`, or `None` for invalid flags.
pub fn open_rights(flags: u32) -> Option<Rights> {
    if flags & !OPEN_MASK != 0 {
        return None;
    }
    let mut rights = Rights::DUP;
    if flags & OPEN_READ != 0 {
        rights = rights.union(Rights::READ);
    }
    if flags & OPEN_WRITE != 0 {
        rights = rights.union(Rights::WRITE);
    }
    if !rights.contains(Rights::READ) && !rights.contains(Rights::WRITE) {
        return None;
    }
    Some(rights)
}

/// True when a server response status is a positive errno or zero.
pub fn valid_status(status: i32) -> bool {
    (0..=4095).contains(&status)
}

#[cfg(test)]
#[path = "model_ut.rs"]
mod tests;
