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
mod tests {
    use super::*;

    fn table() -> MountTable {
        let mut table = MountTable::new();
        table.mount("/", "initfs:").expect("root");
        table
    }

    #[test]
    fn root_resolves_whole_tree() {
        let table = table();
        assert_eq!(table.resolve("/").expect("root"), (1, ""));
        assert_eq!(
            table.resolve("/etc/mounts").expect("file"),
            (1, "etc/mounts")
        );
        assert_eq!(table.resolve("/a/b/c").expect("deep"), (1, "a/b/c"));
    }

    #[test]
    fn longest_prefix_wins() {
        let mut table = table();
        let tmp = table.mount("/tmp", "ramfs:").expect("tmp");
        let nested = table.mount("/tmp/x", "zero:").expect("nested");
        assert_eq!(table.resolve("/tmp/a").expect("tmp file"), (tmp, "a"));
        assert_eq!(table.resolve("/tmp").expect("tmp root"), (tmp, ""));
        assert_eq!(table.resolve("/tmp/x").expect("nested root"), (nested, ""));
        assert_eq!(
            table.resolve("/tmp/x/a").expect("nested file"),
            (nested, "a")
        );
        assert_eq!(table.resolve("/tmpx").expect("not tmp"), (1, "tmpx"));
    }

    #[test]
    fn exact_mount_point_maps_to_empty_suffix() {
        let mut table = table();
        table.mount("/tmp", "ramfs:").expect("tmp");
        assert_eq!(table.resolve("/tmp").expect("tmp"), (2, ""));
    }

    #[test]
    fn missing_mount_is_enoent() {
        let table = MountTable::new();
        assert_eq!(table.resolve("/x"), Err(Errno::ENOENT));
    }

    #[test]
    fn relative_or_empty_path_is_invalid() {
        let table = table();
        assert_eq!(table.resolve("a"), Err(Errno::EINVAL));
        assert_eq!(table.resolve(""), Err(Errno::EINVAL));
    }

    #[test]
    fn non_normalized_mount_points_are_rejected() {
        let mut table = MountTable::new();
        assert_eq!(table.mount("tmp", "ramfs:"), Err(Errno::EINVAL));
        assert_eq!(table.mount("/a//b", "ramfs:"), Err(Errno::EINVAL));
        assert_eq!(table.mount("/a/", "ramfs:"), Err(Errno::EINVAL));
        assert_eq!(table.mount("/a/../b", "ramfs:"), Err(Errno::EINVAL));
    }

    #[test]
    fn invalid_schemes_are_rejected() {
        let mut table = MountTable::new();
        for scheme in ["", "ramfs", ":ramfs", "a/b:", "a:b:", "x\0:"] {
            assert_eq!(table.mount("/", scheme), Err(Errno::EINVAL), "{scheme:?}");
        }
    }

    #[test]
    fn duplicate_mount_is_eexist() {
        let mut table = table();
        assert_eq!(table.mount("/", "ramfs:"), Err(Errno::EEXIST));
    }

    #[test]
    fn maximum_mounts_are_enforced() {
        let mut table = MountTable::new();
        for index in 0..MAX_MOUNTS {
            let path = alloc::format!("/m{index}");
            table.mount(&path, "ramfs:").expect("mount");
        }
        assert_eq!(table.len(), MAX_MOUNTS);
        assert_eq!(table.mount("/extra", "ramfs:"), Err(Errno::ENOSPC));
    }

    #[test]
    fn mount_ids_are_stable_and_not_reused() {
        let mut table = table();
        let first = table.mount("/tmp", "ramfs:").expect("tmp");
        let second = table.mount("/var", "ramfs:").expect("var");
        table.umount("/tmp", 0).expect("umount");
        let third = table.mount("/tmp", "ramfs:").expect("remount");
        assert!(first < second && second < third);
        assert_eq!(table.get(first), None);
        assert_eq!(table.get(second).expect("var").path, "/var");
        assert_eq!(table.get(third).expect("tmp").path, "/tmp");
    }

    #[test]
    fn umount_missing_is_enoent() {
        let mut table = table();
        assert_eq!(table.umount("/tmp", 0), Err(Errno::ENOENT));
    }

    #[test]
    fn umount_root_with_others_is_ebusy() {
        let mut table = table();
        table.mount("/tmp", "ramfs:").expect("tmp");
        assert_eq!(table.umount("/", 0), Err(Errno::EBUSY));
    }

    #[test]
    fn umount_root_alone_is_allowed() {
        let mut table = table();
        table.umount("/", 0).expect("root");
        assert!(table.is_empty());
    }

    #[test]
    fn umount_with_open_files_is_ebusy() {
        let mut table = table();
        table.mount("/tmp", "ramfs:").expect("tmp");
        assert_eq!(table.umount("/tmp", 1), Err(Errno::EBUSY));
        table.umount("/tmp", 0).expect("tmp");
    }

    #[test]
    fn umount_requires_normalized_path() {
        let mut table = table();
        table.mount("/tmp", "ramfs:").expect("tmp");
        assert_eq!(table.umount("tmp", 0), Err(Errno::EINVAL));
        assert_eq!(table.umount("/tmp/", 0), Err(Errno::EINVAL));
    }

    #[test]
    fn rename_same_mount_is_ok() {
        let mut table = table();
        table.mount("/tmp", "ramfs:").expect("tmp");
        assert_eq!(table.rename("/tmp/a", "/tmp/b"), Ok(()));
    }

    #[test]
    fn rename_across_mounts_is_exdev() {
        let mut table = table();
        table.mount("/tmp", "ramfs:").expect("tmp");
        assert_eq!(table.rename("/tmp/a", "/etc/a"), Err(Errno::EXDEV));
    }

    #[test]
    fn rename_missing_path_is_enoent() {
        let table = MountTable::new();
        assert_eq!(table.rename("/nope", "/also"), Err(Errno::ENOENT));
    }

    #[test]
    fn mounts_below_root_lists_only_direct_children() {
        let mut table = table();
        table.mount("/tmp", "ramfs:").expect("tmp");
        table.mount("/dev/null", "null:").expect("null");
        table.mount("/tmp/x", "zero:").expect("nested");
        let children: Vec<&str> = table
            .mounts_below("/")
            .iter()
            .map(|mount| mount.path.as_str())
            .collect();
        assert_eq!(children, ["/tmp"]);
    }

    #[test]
    fn mounts_below_nested_lists_direct_children() {
        let mut table = table();
        table.mount("/tmp", "ramfs:").expect("tmp");
        table.mount("/tmp/x", "zero:").expect("x");
        table.mount("/tmp/x/y", "rand:").expect("y");
        let children: Vec<&str> = table
            .mounts_below("/tmp")
            .iter()
            .map(|mount| mount.path.as_str())
            .collect();
        assert_eq!(children, ["/tmp/x"]);
    }

    #[test]
    fn mounts_below_missing_path_is_empty() {
        let table = table();
        assert!(table.mounts_below("/none").is_empty());
    }

    #[test]
    fn defaults_are_mounted() {
        let mut table = MountTable::new();
        assert_eq!(table.mount_defaults(), Ok(DEFAULT_MOUNTS.len()));
        assert_eq!(table.len(), DEFAULT_MOUNTS.len());
        assert_eq!(table.resolve("/tmp/a").expect("tmp").0, 2);
        assert_eq!(table.resolve("/dev/null").expect("null").0, 3);
    }

    #[test]
    fn parse_mounts_accepts_comments_and_blank_lines() {
        let parsed =
            parse_mounts("# table\n\n/ initfs:\n/tmp ramfs: # trailing\n/dev/null null:\n")
                .expect("parse");
        assert_eq!(
            parsed,
            vec![
                ("/".to_string(), "initfs:".to_string()),
                ("/tmp".to_string(), "ramfs:".to_string()),
                ("/dev/null".to_string(), "null:".to_string()),
            ]
        );
    }

    #[test]
    fn parse_mounts_rejects_bad_lines() {
        assert_eq!(parse_mounts("/\n"), Err(Errno::EINVAL));
        assert_eq!(parse_mounts("/ initfs: extra\n"), Err(Errno::EINVAL));
        assert_eq!(parse_mounts("tmp ramfs:\n"), Err(Errno::EINVAL));
        assert_eq!(parse_mounts("/ ramfs\n"), Err(Errno::EINVAL));
    }

    #[test]
    fn parse_mounts_rejects_duplicate_paths() {
        assert_eq!(parse_mounts("/ initfs:\n/ ramfs:\n"), Err(Errno::EEXIST));
    }

    #[test]
    fn parse_mounts_normalizes_nothing() {
        assert_eq!(parse_mounts("// initfs:\n"), Err(Errno::EINVAL));
    }

    #[test]
    fn normalized_rejects_relative_and_normalizes_dots() {
        assert_eq!(normalized("a"), Err(Errno::EINVAL));
        assert_eq!(normalized("/a/./b/..").expect("normalized"), "/a");
    }

    #[test]
    fn valid_scheme_requires_trailing_colon() {
        assert!(valid_scheme("ramfs:"));
        assert!(valid_scheme("a-b_c:"));
        for scheme in ["", ":", "ramfs", "a/b:", "a:b:"] {
            assert!(!valid_scheme(scheme), "{scheme:?}");
        }
    }

    #[test]
    fn root_only_table_resolves_root() {
        let table = table();
        assert_eq!(table.resolve("/").expect("root"), (1, ""));
        assert_eq!(table.resolve("/x").expect("x"), (1, "x"));
    }

    #[test]
    fn nested_mount_is_reachable_after_parent_removal() {
        let mut table = table();
        table.mount("/tmp", "ramfs:").expect("tmp");
        let nested = table.mount("/tmp/x", "zero:").expect("nested");
        table.umount("/tmp", 0).expect("umount");
        assert_eq!(table.resolve("/tmp/x/a").expect("nested"), (nested, "a"));
    }

    #[test]
    fn mount_scheme_is_returned() {
        let mut table = table();
        table.mount("/tmp", "ramfs:").expect("tmp");
        assert_eq!(table.get(2).expect("tmp").scheme, "ramfs:");
    }

    #[test]
    fn mount_id_overflow_is_enfile() {
        let mut table = MountTable::new();
        table.next_id = u32::MAX - 1;
        assert_eq!(table.mount("/a", "ramfs:"), Ok(u32::MAX - 1));
        assert_eq!(table.mount("/b", "ramfs:"), Err(Errno::ENFILE));
    }

    #[test]
    fn mount_below_exact_equals_itself() {
        let mut table = table();
        table.mount("/tmp", "ramfs:").expect("tmp");
        table.mount("/tmp/x", "zero:").expect("nested");
        assert!(table.mounts_below("/tmp").iter().all(|m| m.path != "/tmp"));
    }

    #[test]
    fn default_table_is_empty_and_starts_at_one() {
        let mut table = MountTable::default();
        assert!(table.is_empty());
        assert_eq!(table.mount("/", "initfs:"), Ok(1));
    }

    #[test]
    fn prefix_without_a_component_boundary_does_not_match() {
        let mut table = table();
        table.mount("/tmp", "ramfs:").expect("tmp");
        assert_eq!(table.resolve("/tm").expect("root path"), (1, "tm"));
    }

    #[test]
    fn parse_mounts_empty_text_is_empty() {
        assert_eq!(parse_mounts(""), Ok(Vec::new()));
        assert_eq!(parse_mounts("  \n# only\n"), Ok(Vec::new()));
    }

    #[test]
    fn parse_mounts_inline_comment_is_ignored() {
        assert_eq!(
            parse_mounts("/ initfs: # root\n"),
            Ok(vec![("/".to_string(), "initfs:".to_string())])
        );
    }

    #[test]
    fn umount_one_mount_keeps_the_others() {
        let mut table = table();
        table.mount("/tmp", "ramfs:").expect("tmp");
        table.mount("/var", "ramfs:").expect("var");
        table.umount("/tmp", 0).expect("tmp");
        assert_eq!(table.resolve("/var/a").expect("var").0, 3);
        assert_eq!(table.resolve("/tmp/a").expect("root").0, 1);
    }

    #[test]
    fn mounts_iterator_is_in_id_order() {
        let mut table = table();
        table.mount("/tmp", "ramfs:").expect("tmp");
        table.mount("/var", "ramfs:").expect("var");
        let ids: Vec<u32> = table.mounts().map(|mount| mount.id).collect();
        assert_eq!(ids, [1, 2, 3]);
    }

    #[test]
    fn rename_same_path_is_ok() {
        let mut table = table();
        table.mount("/tmp", "ramfs:").expect("tmp");
        assert_eq!(table.rename("/tmp/a", "/tmp/a"), Ok(()));
    }

    #[test]
    fn rename_exact_mount_points_on_different_mounts_is_exdev() {
        let mut table = table();
        table.mount("/tmp", "ramfs:").expect("tmp");
        assert_eq!(table.rename("/tmp", "/"), Err(Errno::EXDEV));
    }

    #[test]
    fn mount_defaults_twice_is_eexist() {
        let mut table = MountTable::new();
        table.mount_defaults().expect("defaults");
        assert_eq!(table.mount_defaults(), Err(Errno::EEXIST));
    }
}
