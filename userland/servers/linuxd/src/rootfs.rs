// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The staged Linux root tree as the personality sees it.
//!
//! The B32 recipe packs the Alpine regular files under `/compat/linux/...` in the initfs; the
//! initfs has no directory entries, so this module derives the implicit directories and gives
//! every node a deterministic inode, mode and size. The tree is metadata-only: file contents stay
//! in the initfs and are not served by `open` here.

use alloc::collections::BTreeMap;
use alloc::vec::Vec;

use raven_linux_abi::flags::stat::{S_IFDIR, S_IFREG};

use crate::table::DirEntry;

/// Linux `DT_REG`.
pub const DT_REG: u8 = 8;
/// Linux `DT_DIR`.
pub const DT_DIR: u8 = 4;

/// One regular file staged by the recipe.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RootEntry {
    /// Absolute path inside the Linux root, for example `/compat/linux/bin/busybox`.
    pub path: Vec<u8>,
    /// File size in bytes.
    pub size: u64,
    /// Unix permission bits from the initfs entry.
    pub mode: u32,
}

/// One resolved node of the root tree.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Node {
    /// `DT_REG` or `DT_DIR`.
    pub kind: u8,
    /// File size; 4096 for directories.
    pub size: u64,
    /// `S_IF*` type bits plus permission bits.
    pub mode: u32,
    /// Deterministic inode number.
    pub ino: u64,
}

impl Node {
    /// True when the node is a directory.
    #[must_use]
    pub fn is_dir(&self) -> bool {
        self.kind == DT_DIR
    }
}

/// An in-memory view of the staged Linux root.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RootTree {
    nodes: BTreeMap<Vec<u8>, Node>,
    next_ino: u64,
}

impl RootTree {
    /// Build the tree from the recipe's regular files.
    ///
    /// Every parent directory is created; `/`, `/proc` and `/sys` are always present so a bare
    /// `ls /` has the expected shape. The input is sorted first, so inode numbers do not depend on
    /// the initfs entry order.
    #[must_use]
    pub fn new(entries: &[RootEntry]) -> Self {
        let mut tree = Self {
            nodes: BTreeMap::new(),
            next_ino: 1,
        };
        tree.insert_dir(b"/", 0o755);
        tree.insert_dir(b"/proc", 0o555);
        tree.insert_dir(b"/sys", 0o555);
        let mut sorted: Vec<&RootEntry> = entries.iter().collect();
        sorted.sort_by(|left, right| left.path.cmp(&right.path));
        for entry in sorted {
            tree.insert_file(&entry.path, entry.size, entry.mode);
        }
        tree
    }

    /// True when the tree contains no staged files (only the fixed root directories).
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.nodes.len() <= 3
    }

    /// Resolve one absolute path.
    #[must_use]
    pub fn lookup(&self, path: &[u8]) -> Option<Node> {
        let path = normalize(path)?;
        self.nodes.get(&path).copied()
    }

    /// Direct children of one absolute directory path, sorted by name.
    #[must_use]
    pub fn entries(&self, path: &[u8]) -> Option<Vec<DirEntry>> {
        let path = normalize(path)?;
        let node = self.nodes.get(&path)?;
        if !node.is_dir() {
            return None;
        }
        let mut prefix = path.clone();
        if prefix != b"/" {
            prefix.push(b'/');
        }
        let mut entries = Vec::new();
        for (candidate, child) in &self.nodes {
            if candidate == &path || !candidate.starts_with(&prefix) {
                continue;
            }
            let rest = candidate.get(prefix.len()..)?;
            if rest.contains(&b'/') || rest.is_empty() {
                continue;
            }
            entries.push(DirEntry {
                name: rest.to_vec(),
                kind: child.kind,
                ino: child.ino,
            });
        }
        entries.sort_by(|left, right| left.name.cmp(&right.name));
        Some(entries)
    }

    fn insert_dir(&mut self, path: &[u8], permissions: u32) {
        if self.nodes.contains_key(path) {
            return;
        }
        let ino = self.take_ino();
        self.nodes.insert(
            path.to_vec(),
            Node {
                kind: DT_DIR,
                size: 4096,
                mode: S_IFDIR | permissions,
                ino,
            },
        );
    }

    fn insert_file(&mut self, path: &[u8], size: u64, permissions: u32) {
        if path.first() != Some(&b'/') {
            return;
        }
        let Some(parent) = parent_dir(path) else {
            return;
        };
        self.insert_dirs(&parent);
        if self.nodes.contains_key(path) {
            return;
        }
        let ino = self.take_ino();
        self.nodes.insert(
            path.to_vec(),
            Node {
                kind: DT_REG,
                size,
                mode: S_IFREG | (permissions & 0o7777),
                ino,
            },
        );
    }

    fn insert_dirs(&mut self, path: &[u8]) {
        let mut cursor = Vec::new();
        for component in path.split(|byte| *byte == b'/') {
            if component.is_empty() {
                continue;
            }
            cursor.push(b'/');
            cursor.extend_from_slice(component);
            self.insert_dir(&cursor, 0o755);
        }
    }

    fn take_ino(&mut self) -> u64 {
        let ino = self.next_ino;
        self.next_ino = self.next_ino.saturating_add(1);
        ino
    }
}

fn normalize(path: &[u8]) -> Option<Vec<u8>> {
    if path.first() != Some(&b'/') {
        return None;
    }
    let trimmed = match path.len() {
        0 => return None,
        1 => return Some(b"/".to_vec()),
        _ => {
            let mut end = path.len();
            while end > 1 && path.get(end - 1) == Some(&b'/') {
                end -= 1;
            }
            path.get(..end)?
        }
    };
    Some(trimmed.to_vec())
}

fn parent_dir(path: &[u8]) -> Option<Vec<u8>> {
    let mut end = path.len();
    while end > 1 && path.get(end - 1) == Some(&b'/') {
        end -= 1;
    }
    let trimmed = path.get(..end)?;
    let slash = trimmed.iter().rposition(|byte| *byte == b'/')?;
    if slash == 0 {
        Some(b"/".to_vec())
    } else {
        Some(trimmed.get(..slash)?.to_vec())
    }
}

#[cfg(test)]
#[path = "rootfs_ut.rs"]
mod tests;
