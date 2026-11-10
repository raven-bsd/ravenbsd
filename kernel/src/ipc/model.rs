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

    /// Allow every scheme name beginning with `prefix`.
    #[allow(dead_code)] // exercised by the host model; no namespace syscall sets prefixes yet
    pub fn allow_prefix(&mut self, prefix: &str) {
        if !prefix.is_empty() && !self.prefixes.iter().any(|item| item == prefix) {
            self.prefixes.push(prefix.to_string());
        }
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
mod tests {
    use super::*;
    use alloc::format;

    #[test]
    fn scheme_names_follow_the_contract() {
        for name in ["echo", "file", "net", "a", "a-b_c:0"] {
            assert!(valid_scheme_name(name), "{name}");
        }
        for name in ["", "Echo", "a b", "a/b", "ä", &"a".repeat(65)] {
            assert!(!valid_scheme_name(name), "{name:?}");
        }
    }

    #[test]
    fn paths_split_at_the_first_colon() {
        assert_eq!(split_path("echo:hello"), Ok(("echo", "hello")));
        assert_eq!(split_path("echo:"), Ok(("echo", "")));
        assert_eq!(split_path("file:/etc/passwd"), Ok(("file", "/etc/passwd")));
        assert_eq!(split_path("echo"), Err(PathError::MissingScheme));
        assert_eq!(split_path(""), Err(PathError::BadLength));
        assert_eq!(split_path("Echo:x"), Err(PathError::BadScheme));
        assert_eq!(
            split_path(&format!("echo:{}", "x".repeat(PATH_MAX))),
            Err(PathError::BadLength)
        );
    }

    #[test]
    fn root_and_child_namespaces() {
        let root = Namespace::root();
        assert!(root.allows("anything"));
        let registered = alloc::vec!["echo".to_string()];
        let child = root.child_snapshot(&registered);
        assert!(!child.wildcard);
        assert!(child.allows("echo"));
        assert!(!child.allows("file"));

        let mut child = child;
        child.add("file");
        assert!(child.allows("file"));
        child.add("file");
        assert_eq!(child.names.len(), 2);
        let grandchild = child.child_snapshot(&[]);
        assert!(grandchild.allows("echo"));
        assert!(grandchild.allows("file"));

        let mut prefixed = Namespace::default();
        prefixed.allow_prefix("net");
        assert!(prefixed.allows("net"));
        assert!(prefixed.allows("net-tcp"));
        assert!(!prefixed.allows("file"));
        let inherited = prefixed.child_snapshot(&[]);
        assert!(inherited.allows("net-tcp"));
    }

    #[test]
    fn open_rights_are_derived_from_flags() {
        let read = open_rights(OPEN_READ).unwrap();
        assert!(read.contains(Rights::READ));
        assert!(!read.contains(Rights::WRITE));
        assert!(read.contains(Rights::DUP));

        let write = open_rights(OPEN_WRITE).unwrap();
        assert!(write.contains(Rights::WRITE));
        assert!(!write.contains(Rights::READ));

        let both = open_rights(OPEN_READ | OPEN_WRITE | OPEN_CREATE).unwrap();
        assert!(both.contains(Rights::READ.union(Rights::WRITE).union(Rights::DUP)));

        assert_eq!(open_rights(0), None);
        assert_eq!(open_rights(8), None);
    }

    #[test]
    fn status_range_is_positive_errno_only() {
        assert!(valid_status(0));
        assert!(valid_status(5));
        assert!(valid_status(4095));
        assert!(!valid_status(-1));
        assert!(!valid_status(4096));
    }

    #[test]
    fn random_paths_never_panic() {
        let mut state = 0x9e37_79b9_7f4a_7c15u64;
        let mut text = String::new();
        for _ in 0..10_000 {
            text.clear();
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            let length = (state % 32) as usize;
            for _ in 0..length {
                state ^= state << 13;
                state ^= state >> 7;
                state ^= state << 17;
                text.push(char::from(b'a' + (state % 26) as u8));
            }
            let _ = valid_scheme_name(&text);
            let _ = split_path(&text);
        }
    }
}
