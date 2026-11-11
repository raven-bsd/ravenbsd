// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Pure in-memory filesystem model for `ramfsd`.
//!
//! The model owns an inode table, directory trees, symlinks and file data. It has no syscalls,
//! threads or global state: the server supplies the current time and maps K60 requests onto these
//! methods. Paths are interpreted relative to a base directory inode; a leading `/` always means
//! the filesystem root.
//!
//! Hard links are not supported. Limiting and error behaviour:
//!
//! * file data beyond the configured limit is `ENOSPC` and is rejected before growing a vector;
//! * the inode limit and path-depth limit are `ENOSPC`/`ENOMEM`;
//! * names longer than the configured limit are `ENAMETOOLONG`;
//! * names must not be empty, contain `/` or NUL, or be `.`/`..`; they are `EINVAL`.
//!
//! Unlinking an inode leaves it with `nlink == 0` until the server calls [`Ramfs::release`] when
//! the last open handle closes. That is the open-unlink lifetime; [`Ramfs::audit`] checks that
//! every retained inode is either reachable or unlinked.

#![cfg_attr(not(test), no_std)]
#![forbid(unsafe_code)]
#![warn(missing_docs)]

extern crate alloc;

use alloc::collections::{BTreeMap, BTreeSet, VecDeque};
use alloc::string::{String, ToString};
use alloc::vec;
use alloc::vec::Vec;

use raven_proto_fs::{
    DT_DIR, DT_LNK, DT_REG, DirEntry, DirEntryHeader, Errno, S_IFDIR, S_IFLNK, S_IFREG, Stat,
    StatFs,
};

/// Inode number of the root directory.
pub const ROOT_INO: u64 = 1;
/// Logical block size reported by [`Ramfs::statfs`] and used for `st_blocks`.
pub const BLOCK_SIZE: u32 = 4096;
/// Maximum number of symlink expansions in one lookup.
const MAX_SYMLINK_EXPANSIONS: usize = 8;

/// Resource limits for one filesystem instance.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Limits {
    /// Maximum total file data in bytes.
    pub max_bytes: usize,
    /// Maximum number of inodes, including the root.
    pub max_inodes: usize,
    /// Maximum name length in bytes.
    pub max_name: usize,
    /// Maximum path depth in components.
    pub max_depth: usize,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            max_bytes: 16 * 1024 * 1024,
            max_inodes: 4096,
            max_name: 255,
            max_depth: 32,
        }
    }
}

/// The payload of one inode.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NodeKind {
    /// A regular file with its bytes.
    File(Vec<u8>),
    /// A directory mapping names to inode numbers.
    Dir(BTreeMap<String, u64>),
    /// A symbolic link target.
    Symlink(Vec<u8>),
}

/// One inode with its metadata.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Node {
    /// File, directory or symlink payload.
    pub kind: NodeKind,
    /// Permission bits, without the file type.
    pub mode: u16,
    /// Link count; zero means unlinked but still open.
    pub nlink: u32,
    /// Modification time, seconds from the caller's clock.
    pub mtime: i64,
    /// Change time, seconds from the caller's clock.
    pub ctime: i64,
    /// Access time, seconds from the caller's clock.
    pub atime: i64,
}

/// An in-memory filesystem.
#[derive(Debug)]
pub struct Ramfs {
    nodes: BTreeMap<u64, Node>,
    next_ino: u64,
    total_bytes: usize,
    limits: Limits,
}

impl Ramfs {
    /// Create a filesystem with default limits and a root directory stamped with `now`.
    pub fn new(now: i64) -> Self {
        Self::with_limits(Limits::default(), now)
    }

    /// Create a filesystem with explicit limits.
    pub fn with_limits(limits: Limits, now: i64) -> Self {
        let mut nodes = BTreeMap::new();
        nodes.insert(
            ROOT_INO,
            Node {
                kind: NodeKind::Dir(BTreeMap::new()),
                mode: 0o755,
                nlink: 2,
                mtime: now,
                ctime: now,
                atime: now,
            },
        );
        Self {
            nodes,
            next_ino: ROOT_INO + 1,
            total_bytes: 0,
            limits,
        }
    }

    /// Configured limits.
    pub const fn limits(&self) -> Limits {
        self.limits
    }

    /// Number of retained inodes, including unlinked open ones.
    pub fn node_count(&self) -> usize {
        self.nodes.len()
    }

    /// Total retained file bytes.
    pub const fn total_bytes(&self) -> usize {
        self.total_bytes
    }

    /// True when `ino` exists with `nlink == 0`.
    pub fn is_unlinked(&self, ino: u64) -> bool {
        self.nodes.get(&ino).is_some_and(|node| node.nlink == 0)
    }

    /// Look up `path` relative to `base`, following a final symlink.
    pub fn lookup_at(&self, base: u64, path: &str) -> Result<u64, Errno> {
        self.resolve(base, path, true)
    }

    /// Look up `path` relative to `base`, without following a final symlink.
    pub fn lookup_no_follow_at(&self, base: u64, path: &str) -> Result<u64, Errno> {
        self.resolve(base, path, false)
    }

    /// Look up `path` from the root, following a final symlink.
    pub fn lookup(&self, path: &str) -> Result<u64, Errno> {
        self.lookup_at(ROOT_INO, path)
    }

    /// Look up `path` from the root without following a final symlink.
    pub fn lookup_no_follow(&self, path: &str) -> Result<u64, Errno> {
        self.lookup_no_follow_at(ROOT_INO, path)
    }

    /// Create a regular file at `path` relative to `base`.
    pub fn create_file_at(
        &mut self,
        base: u64,
        path: &str,
        mode: u16,
        now: i64,
    ) -> Result<u64, Errno> {
        self.create_at(base, path, NodeKind::File(Vec::new()), mode & 0o7777, now)
    }

    /// Create a directory at `path` relative to `base`.
    pub fn mkdir_at(&mut self, base: u64, path: &str, mode: u16, now: i64) -> Result<u64, Errno> {
        self.create_at(
            base,
            path,
            NodeKind::Dir(BTreeMap::new()),
            mode & 0o7777,
            now,
        )
    }

    /// Create a symbolic link at `path` relative to `base` pointing to `target`.
    pub fn symlink_at(
        &mut self,
        base: u64,
        path: &str,
        target: &[u8],
        now: i64,
    ) -> Result<u64, Errno> {
        let target = core::str::from_utf8(target).map_err(|_| Errno::EINVAL)?;
        if target.is_empty() || target.len() > self.limits.max_name || target.contains('\0') {
            return Err(Errno::EINVAL);
        }
        self.create_at(
            base,
            path,
            NodeKind::Symlink(target.as_bytes().to_vec()),
            0o777,
            now,
        )
    }

    /// Read from a file at `offset`; updates atime to `now`.
    pub fn read(
        &mut self,
        ino: u64,
        offset: u64,
        out: &mut [u8],
        now: i64,
    ) -> Result<usize, Errno> {
        let start = usize::try_from(offset).map_err(|_| Errno::EINVAL)?;
        let Some(node) = self.nodes.get_mut(&ino) else {
            return Err(Errno::EBADF);
        };
        let NodeKind::File(data) = &node.kind else {
            return Err(if matches!(&node.kind, NodeKind::Dir(_)) {
                Errno::EISDIR
            } else {
                Errno::EINVAL
            });
        };
        let available = data.get(start..).unwrap_or(&[]);
        let length = available.len().min(out.len());
        let target = out.get_mut(..length).ok_or(Errno::EINVAL)?;
        target.copy_from_slice(available.get(..length).unwrap_or(&[]));
        node.atime = now;
        Ok(length)
    }

    /// Write `data` at `offset`, zero-filling a gap and extending the file.
    pub fn write(&mut self, ino: u64, offset: u64, data: &[u8], now: i64) -> Result<usize, Errno> {
        let start = usize::try_from(offset).map_err(|_| Errno::EINVAL)?;
        let Some(node) = self.nodes.get_mut(&ino) else {
            return Err(Errno::EBADF);
        };
        let NodeKind::File(file) = &mut node.kind else {
            return Err(if matches!(&node.kind, NodeKind::Dir(_)) {
                Errno::EISDIR
            } else {
                Errno::EINVAL
            });
        };
        if data.is_empty() {
            return Ok(0);
        }
        let end = start.checked_add(data.len()).ok_or(Errno::ENOMEM)?;
        let old_len = file.len();
        let new_len = old_len.max(end);
        let growth = new_len.saturating_sub(old_len);
        if self
            .total_bytes
            .checked_add(growth)
            .is_none_or(|total| total > self.limits.max_bytes)
        {
            return Err(Errno::ENOSPC);
        }
        if file.try_reserve(growth).is_err() {
            return Err(Errno::ENOMEM);
        }
        if new_len > old_len {
            file.resize(new_len, 0);
        }
        if let Some(slot) = file.get_mut(start..end) {
            slot.copy_from_slice(data);
        }
        self.total_bytes += growth;
        node.mtime = now;
        node.ctime = now;
        Ok(data.len())
    }

    /// Truncate or extend a file to `size` bytes.
    pub fn truncate(&mut self, ino: u64, size: u64, now: i64) -> Result<(), Errno> {
        let size = usize::try_from(size).map_err(|_| Errno::ENOMEM)?;
        let Some(node) = self.nodes.get_mut(&ino) else {
            return Err(Errno::EBADF);
        };
        let NodeKind::File(file) = &mut node.kind else {
            return Err(if matches!(&node.kind, NodeKind::Dir(_)) {
                Errno::EISDIR
            } else {
                Errno::EINVAL
            });
        };
        if size > file.len() {
            let growth = size - file.len();
            if self
                .total_bytes
                .checked_add(growth)
                .is_none_or(|total| total > self.limits.max_bytes)
            {
                return Err(Errno::ENOSPC);
            }
            if file.try_reserve(growth).is_err() {
                return Err(Errno::ENOMEM);
            }
            file.resize(size, 0);
            self.total_bytes += growth;
        } else {
            let shrink = file.len() - size;
            file.truncate(size);
            self.total_bytes = self.total_bytes.saturating_sub(shrink);
        }
        node.mtime = now;
        node.ctime = now;
        Ok(())
    }

    /// File length, or the target length for a symlink.
    pub fn len(&self, ino: u64) -> Result<u64, Errno> {
        let node = self.nodes.get(&ino).ok_or(Errno::EBADF)?;
        Ok(match &node.kind {
            NodeKind::File(data) => data.len() as u64,
            NodeKind::Symlink(target) => target.len() as u64,
            NodeKind::Dir(_) => 0,
        })
    }

    /// Stat one inode.
    pub fn stat(&self, ino: u64) -> Result<Stat, Errno> {
        let node = self.nodes.get(&ino).ok_or(Errno::EBADF)?;
        let (mode, size) = match &node.kind {
            NodeKind::File(data) => (S_IFREG | u32::from(node.mode), data.len() as u64),
            NodeKind::Dir(_) => (S_IFDIR | u32::from(node.mode), 0),
            NodeKind::Symlink(target) => (S_IFLNK | u32::from(node.mode), target.len() as u64),
        };
        Ok(Stat {
            ino,
            size,
            blocks: size.div_ceil(512),
            mtime: node.mtime,
            ctime: node.ctime,
            atime: node.atime,
            mode,
            nlink: node.nlink,
            uid: 0,
            gid: 0,
        })
    }

    /// Filesystem statistics.
    pub fn statfs(&self) -> StatFs {
        let total_blocks = (self.limits.max_bytes / BLOCK_SIZE as usize) as u64;
        let used_blocks = (self.total_bytes.div_ceil(BLOCK_SIZE as usize)) as u64;
        StatFs {
            block_size: BLOCK_SIZE,
            blocks: total_blocks,
            free_blocks: total_blocks.saturating_sub(used_blocks),
            files: self.limits.max_inodes as u64,
            free_files: (self.limits.max_inodes.saturating_sub(self.nodes.len())) as u64,
        }
    }

    /// Change permission bits of `path` relative to `base`.
    pub fn chmod_at(&mut self, base: u64, path: &str, mode: u16, now: i64) -> Result<(), Errno> {
        let ino = self.resolve(base, path, true)?;
        let node = self.nodes.get_mut(&ino).ok_or(Errno::EBADF)?;
        node.mode = mode & 0o7777;
        node.ctime = now;
        Ok(())
    }

    /// Remove a file or symlink `path` relative to `base`; returns the unlinked inode.
    pub fn unlink_at(&mut self, base: u64, path: &str, now: i64) -> Result<u64, Errno> {
        let (parent_ino, name) = self.parent_and_name(base, path)?;
        let ino = {
            let parent = self.nodes.get(&parent_ino).ok_or(Errno::EBADF)?;
            let NodeKind::Dir(entries) = &parent.kind else {
                return Err(Errno::ENOTDIR);
            };
            *entries.get(&name).ok_or(Errno::ENOENT)?
        };
        if matches!(
            self.nodes.get(&ino).ok_or(Errno::EBADF)?.kind,
            NodeKind::Dir(_)
        ) {
            return Err(Errno::EISDIR);
        }
        {
            let parent = self.nodes.get_mut(&parent_ino).ok_or(Errno::EBADF)?;
            let NodeKind::Dir(entries) = &mut parent.kind else {
                return Err(Errno::ENOTDIR);
            };
            entries.remove(&name);
            parent.mtime = now;
            parent.ctime = now;
        }
        if let Some(node) = self.nodes.get_mut(&ino) {
            node.nlink = 0;
        }
        Ok(ino)
    }

    /// Remove an empty directory `path` relative to `base`; returns the unlinked inode.
    pub fn rmdir_at(&mut self, base: u64, path: &str, now: i64) -> Result<u64, Errno> {
        let (parent_ino, name) = self.parent_and_name(base, path)?;
        let ino = {
            let parent = self.nodes.get(&parent_ino).ok_or(Errno::EBADF)?;
            let NodeKind::Dir(entries) = &parent.kind else {
                return Err(Errno::ENOTDIR);
            };
            *entries.get(&name).ok_or(Errno::ENOENT)?
        };
        let Some(node) = self.nodes.get(&ino) else {
            return Err(Errno::EBADF);
        };
        let NodeKind::Dir(child) = &node.kind else {
            return Err(Errno::ENOTDIR);
        };
        if !child.is_empty() {
            return Err(Errno::ENOTEMPTY);
        }
        {
            let parent = self.nodes.get_mut(&parent_ino).ok_or(Errno::EBADF)?;
            let NodeKind::Dir(entries) = &mut parent.kind else {
                return Err(Errno::ENOTDIR);
            };
            entries.remove(&name);
            parent.nlink = parent.nlink.saturating_sub(1);
            parent.mtime = now;
            parent.ctime = now;
        }
        if let Some(node) = self.nodes.get_mut(&ino) {
            node.nlink = 0;
        }
        Ok(ino)
    }

    /// Rename `from` to `to`, each relative to `base` unless absolute.
    ///
    /// Returns the replaced inode, if any, with `nlink == 0`; the server releases it when its last
    /// open handle closes.
    pub fn rename_at(
        &mut self,
        base: u64,
        from: &str,
        to: &str,
        now: i64,
    ) -> Result<Option<u64>, Errno> {
        let (source_parent, source_name) = self.parent_and_name(base, from)?;
        let (dest_parent, dest_name) = self.parent_and_name(base, to)?;
        let source_ino = {
            let node = self.nodes.get(&source_parent).ok_or(Errno::EBADF)?;
            let NodeKind::Dir(entries) = &node.kind else {
                return Err(Errno::ENOTDIR);
            };
            *entries.get(&source_name).ok_or(Errno::ENOENT)?
        };
        if source_ino == ROOT_INO {
            return Err(Errno::EINVAL);
        }
        if source_parent == dest_parent && source_name == dest_name {
            return Ok(None);
        }
        let source_is_dir = matches!(
            self.nodes.get(&source_ino).ok_or(Errno::EBADF)?.kind,
            NodeKind::Dir(_)
        );
        if source_is_dir && self.is_descendant(source_ino, dest_parent) {
            return Err(Errno::EINVAL);
        }
        let dest_ino = {
            let node = self.nodes.get(&dest_parent).ok_or(Errno::EBADF)?;
            let NodeKind::Dir(entries) = &node.kind else {
                return Err(Errno::ENOTDIR);
            };
            entries.get(&dest_name).copied()
        };
        if dest_ino == Some(source_ino) {
            return Ok(None);
        }
        let mut replaced = None;
        if let Some(dest_ino) = dest_ino {
            let dest_kind = self.nodes.get(&dest_ino).ok_or(Errno::EBADF)?.kind.clone();
            match (&dest_kind, source_is_dir) {
                (NodeKind::Dir(entries), true) => {
                    if !entries.is_empty() {
                        return Err(Errno::ENOTEMPTY);
                    }
                }
                (NodeKind::Dir(_), false) => return Err(Errno::EISDIR),
                (_, true) => return Err(Errno::ENOTDIR),
                _ => {}
            }
            if let Some(node) = self.nodes.get_mut(&dest_ino) {
                node.nlink = 0;
            }
            if matches!(&dest_kind, NodeKind::Dir(_))
                && let Some(parent) = self.nodes.get_mut(&dest_parent)
            {
                parent.nlink = parent.nlink.saturating_sub(1);
            }
            replaced = Some(dest_ino);
        }
        {
            let node = self.nodes.get_mut(&source_parent).ok_or(Errno::EBADF)?;
            let NodeKind::Dir(entries) = &mut node.kind else {
                return Err(Errno::ENOTDIR);
            };
            entries.remove(&source_name);
            node.mtime = now;
            node.ctime = now;
            if source_is_dir {
                node.nlink = node.nlink.saturating_sub(1);
            }
        }
        {
            let node = self.nodes.get_mut(&dest_parent).ok_or(Errno::EBADF)?;
            let NodeKind::Dir(entries) = &mut node.kind else {
                return Err(Errno::ENOTDIR);
            };
            entries.insert(dest_name, source_ino);
            node.mtime = now;
            node.ctime = now;
            if source_is_dir {
                node.nlink += 1;
            }
        }
        if let Some(node) = self.nodes.get_mut(&source_ino) {
            node.ctime = now;
        }
        Ok(replaced)
    }

    /// Read a symlink target relative to `base`.
    pub fn readlink_at(&self, base: u64, path: &str) -> Result<Vec<u8>, Errno> {
        let ino = self.resolve(base, path, false)?;
        let node = self.nodes.get(&ino).ok_or(Errno::EBADF)?;
        match &node.kind {
            NodeKind::Symlink(target) => Ok(target.clone()),
            NodeKind::Dir(_) => Err(Errno::EISDIR),
            NodeKind::File(_) => Err(Errno::EINVAL),
        }
    }

    /// Read packed K60 directory entries starting at byte `cookie`.
    pub fn readdir(
        &mut self,
        ino: u64,
        cookie: u64,
        out: &mut [u8],
        now: i64,
    ) -> Result<usize, Errno> {
        let entries = {
            let node = self.nodes.get(&ino).ok_or(Errno::EBADF)?;
            let NodeKind::Dir(entries) = &node.kind else {
                return Err(Errno::ENOTDIR);
            };
            entries.clone()
        };
        let mut offset = 0u64;
        let mut written = 0usize;
        for (name, child_ino) in entries.iter() {
            let Some(child) = self.nodes.get(child_ino) else {
                return Err(Errno::EIO);
            };
            let kind = match &child.kind {
                NodeKind::File(_) => DT_REG,
                NodeKind::Dir(_) => DT_DIR,
                NodeKind::Symlink(_) => DT_LNK,
            };
            let entry = DirEntry {
                header: DirEntryHeader {
                    ino: *child_ino,
                    kind,
                    name_len: u8::try_from(name.len()).map_err(|_| Errno::ENAMETOOLONG)?,
                    pad: [0; 6],
                },
                name: name.as_bytes(),
            };
            let length = entry.encoded_len();
            let start = offset;
            offset = offset.checked_add(length as u64).ok_or(Errno::EIO)?;
            if offset <= cookie {
                continue;
            }
            if start < cookie {
                return Err(Errno::EINVAL);
            }
            if written + length > out.len() {
                if written == 0 {
                    return Err(Errno::EINVAL);
                }
                break;
            }
            let slot = out
                .get_mut(written..written + length)
                .ok_or(Errno::EINVAL)?;
            entry.encode(slot).map_err(|_| Errno::EIO)?;
            written += length;
        }
        if let Some(node) = self.nodes.get_mut(&ino) {
            node.atime = now;
        }
        Ok(written)
    }

    /// Drop an unlinked inode when its last open handle closes.
    pub fn release(&mut self, ino: u64) -> Result<(), Errno> {
        let Some(node) = self.nodes.get(&ino) else {
            return Err(Errno::EBADF);
        };
        if node.nlink != 0 {
            return Err(Errno::EINVAL);
        }
        let bytes = match &node.kind {
            NodeKind::File(data) => data.len(),
            _ => 0,
        };
        self.nodes.remove(&ino);
        self.total_bytes = self.total_bytes.saturating_sub(bytes);
        Ok(())
    }

    /// Hard links are not supported in this milestone.
    pub fn link_at(
        &mut self,
        _base: u64,
        _path: &str,
        _target: &str,
        _now: i64,
    ) -> Result<(), Errno> {
        Err(Errno::EPERM)
    }

    /// Check the inode graph and accounting invariants.
    ///
    /// Returns a static description of the first inconsistency. This is used by the host model
    /// test and is harmless to call at runtime.
    pub fn audit(&self) -> Result<(), &'static str> {
        let root = self.nodes.get(&ROOT_INO).ok_or("root inode missing")?;
        if !matches!(root.kind, NodeKind::Dir(_)) {
            return Err("root is not a directory");
        }
        let mut reachable = BTreeSet::new();
        let mut stack = vec![ROOT_INO];
        while let Some(ino) = stack.pop() {
            if !reachable.insert(ino) {
                return Err("directory graph contains a cycle or duplicate");
            }
            let node = self
                .nodes
                .get(&ino)
                .ok_or("directory entry points to a missing inode")?;
            if let NodeKind::Dir(entries) = &node.kind {
                for child in entries.values() {
                    if !self.nodes.contains_key(child) {
                        return Err("directory entry points to a missing inode");
                    }
                    stack.push(*child);
                }
            }
        }
        let mut total = 0usize;
        for (ino, node) in &self.nodes {
            let linked = reachable.contains(ino);
            match &node.kind {
                NodeKind::File(data) => {
                    total = total.checked_add(data.len()).ok_or("byte count overflow")?;
                    let expected = u32::from(linked);
                    if node.nlink != expected {
                        return Err("file link count mismatch");
                    }
                }
                NodeKind::Symlink(_) => {
                    let expected = u32::from(linked);
                    if node.nlink != expected {
                        return Err("symlink link count mismatch");
                    }
                }
                NodeKind::Dir(entries) => {
                    if linked {
                        let subdirs = entries
                            .values()
                            .filter(|child| {
                                self.nodes
                                    .get(child)
                                    .is_some_and(|node| matches!(node.kind, NodeKind::Dir(_)))
                            })
                            .count();
                        let expected =
                            u32::try_from(2 + subdirs).map_err(|_| "link count overflow")?;
                        if node.nlink != expected {
                            return Err("directory link count mismatch");
                        }
                    } else if node.nlink != 0 || !entries.is_empty() {
                        return Err("unlinked directory is not empty");
                    }
                }
            }
        }
        if total != self.total_bytes {
            return Err("total byte count mismatch");
        }
        let _ = root;
        Ok(())
    }

    fn create_at(
        &mut self,
        base: u64,
        path: &str,
        kind: NodeKind,
        mode: u16,
        now: i64,
    ) -> Result<u64, Errno> {
        let (parent_ino, name) = self.parent_and_name(base, path)?;
        if self.nodes.len() >= self.limits.max_inodes {
            return Err(Errno::ENOSPC);
        }
        let ino = self.next_ino;
        self.next_ino = self.next_ino.checked_add(1).ok_or(Errno::ENOMEM)?;
        {
            let parent = self.nodes.get_mut(&parent_ino).ok_or(Errno::EBADF)?;
            let NodeKind::Dir(entries) = &mut parent.kind else {
                return Err(Errno::ENOTDIR);
            };
            if entries.contains_key(&name) {
                return Err(Errno::EEXIST);
            }
            entries.insert(name, ino);
            parent.mtime = now;
            parent.ctime = now;
            if matches!(kind, NodeKind::Dir(_)) {
                parent.nlink += 1;
            }
        }
        let nlink = if matches!(kind, NodeKind::Dir(_)) {
            2
        } else {
            1
        };
        self.nodes.insert(
            ino,
            Node {
                kind,
                mode,
                nlink,
                mtime: now,
                ctime: now,
                atime: now,
            },
        );
        Ok(ino)
    }

    fn parent_and_name(&self, base: u64, path: &str) -> Result<(u64, String), Errno> {
        let trimmed = path.trim_end_matches('/');
        if trimmed.is_empty() || trimmed.contains('\0') {
            return Err(Errno::EINVAL);
        }
        self.components(path)?;
        let (parent_text, name) = match trimmed.rsplit_once('/') {
            Some((parent, name)) => (parent, name),
            None => ("", trimmed),
        };
        if name.is_empty() {
            return Err(Errno::EINVAL);
        }
        self.validate_name(name)?;
        let parent = if path.starts_with('/') {
            self.resolve(
                ROOT_INO,
                if parent_text.is_empty() {
                    "/"
                } else {
                    parent_text
                },
                true,
            )?
        } else {
            self.resolve(base, parent_text, true)?
        };
        if !matches!(
            self.nodes.get(&parent).ok_or(Errno::EBADF)?.kind,
            NodeKind::Dir(_)
        ) {
            return Err(Errno::ENOTDIR);
        }
        Ok((parent, name.to_string()))
    }

    fn validate_name(&self, name: &str) -> Result<(), Errno> {
        if name.is_empty() || name == "." || name == ".." || name.contains('/') {
            return Err(Errno::EINVAL);
        }
        if name.contains('\0') {
            return Err(Errno::EINVAL);
        }
        if name.len() > self.limits.max_name {
            return Err(Errno::ENAMETOOLONG);
        }
        Ok(())
    }

    fn components<'a>(&self, path: &'a str) -> Result<Vec<&'a str>, Errno> {
        if path.contains('\0') {
            return Err(Errno::EINVAL);
        }
        let mut components = Vec::new();
        for component in path.split('/').filter(|component| !component.is_empty()) {
            self.validate_name(component)?;
            components.push(component);
        }
        if components.len() > self.limits.max_depth {
            return Err(Errno::ENOMEM);
        }
        Ok(components)
    }

    fn resolve(&self, base: u64, path: &str, follow_final: bool) -> Result<u64, Errno> {
        if !self.nodes.contains_key(&base) {
            return Err(Errno::EBADF);
        }
        let absolute = path.starts_with('/');
        let mut current = if absolute { ROOT_INO } else { base };
        let mut queue: VecDeque<String> = self
            .components(path)?
            .into_iter()
            .map(|component| component.to_string())
            .collect();
        if queue.is_empty() {
            return Ok(current);
        }
        let mut expansions = 0usize;
        while let Some(name) = queue.pop_front() {
            let parent = current;
            let node = self.nodes.get(&parent).ok_or(Errno::EBADF)?;
            let NodeKind::Dir(entries) = &node.kind else {
                return Err(Errno::ENOTDIR);
            };
            let ino = *entries.get(&name).ok_or(Errno::ENOENT)?;
            let child = self.nodes.get(&ino).ok_or(Errno::EBADF)?;
            if let NodeKind::Symlink(target) = &child.kind {
                let is_last = queue.is_empty();
                if !is_last || follow_final {
                    expansions += 1;
                    if expansions > MAX_SYMLINK_EXPANSIONS {
                        return Err(Errno::ELOOP);
                    }
                    let target = core::str::from_utf8(target).map_err(|_| Errno::EINVAL)?;
                    let target_absolute = target.starts_with('/');
                    let target_components = self.components(target)?;
                    if target_components.len() > self.limits.max_depth {
                        return Err(Errno::ENOMEM);
                    }
                    if target_absolute {
                        current = ROOT_INO;
                    } else {
                        current = parent;
                    }
                    let mut rebuilt = VecDeque::new();
                    for component in target_components {
                        rebuilt.push_back(component.to_string());
                    }
                    rebuilt.append(&mut queue);
                    queue = rebuilt;
                    continue;
                }
            }
            current = ino;
        }
        Ok(current)
    }

    fn is_descendant(&self, ancestor: u64, node: u64) -> bool {
        let mut stack = vec![ancestor];
        let mut seen = BTreeSet::new();
        while let Some(ino) = stack.pop() {
            if ino == node {
                return true;
            }
            if !seen.insert(ino) {
                continue;
            }
            if let Some(Node {
                kind: NodeKind::Dir(entries),
                ..
            }) = self.nodes.get(&ino)
            {
                stack.extend(entries.values().copied());
            }
        }
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::format;
    use alloc::vec;
    use raven_proto_fs::DirEntryIter;

    fn root() -> Ramfs {
        Ramfs::new(100)
    }

    fn small() -> Ramfs {
        Ramfs::with_limits(
            Limits {
                max_bytes: 4096,
                max_inodes: 8,
                max_name: 8,
                max_depth: 4,
            },
            100,
        )
    }

    fn read_all(fs: &mut Ramfs, ino: u64) -> Vec<u8> {
        let length = usize::try_from(fs.len(ino).unwrap()).unwrap();
        let mut data = vec![0u8; length];
        let read = fs.read(ino, 0, &mut data, 200).unwrap();
        data.truncate(read);
        data
    }

    fn names(fs: &mut Ramfs, ino: u64) -> Vec<String> {
        let mut out = [0u8; 1024];
        let length = fs.readdir(ino, 0, &mut out, 1).unwrap();
        DirEntryIter::new(&out[..length])
            .map(|entry| entry.unwrap().name.to_vec())
            .map(|name| String::from_utf8(name).unwrap())
            .collect()
    }

    #[test]
    fn root_stat_is_a_directory() {
        let fs = root();
        let stat = fs.stat(ROOT_INO).unwrap();
        assert_eq!(stat.mode & 0o170000, S_IFDIR);
        assert_eq!(stat.mode & 0o777, 0o755);
        assert_eq!(stat.nlink, 2);
        assert_eq!(fs.node_count(), 1);
    }

    #[test]
    fn create_file_and_lookup() {
        let mut fs = root();
        let ino = fs.create_file_at(ROOT_INO, "/a", 0o644, 1).unwrap();
        assert_eq!(fs.lookup("/a").unwrap(), ino);
        assert_eq!(fs.stat(ino).unwrap().mode, S_IFREG | 0o644);
        assert_eq!(fs.audit(), Ok(()));
    }

    #[test]
    fn create_duplicate_is_eexist() {
        let mut fs = root();
        fs.create_file_at(ROOT_INO, "/a", 0o644, 1).unwrap();
        assert_eq!(
            fs.create_file_at(ROOT_INO, "/a", 0o644, 1),
            Err(Errno::EEXIST)
        );
    }

    #[test]
    fn create_in_missing_parent_is_enoent() {
        let mut fs = root();
        assert_eq!(
            fs.create_file_at(ROOT_INO, "/missing/a", 0o644, 1),
            Err(Errno::ENOENT)
        );
    }

    #[test]
    fn create_under_a_file_is_enotdir() {
        let mut fs = root();
        fs.create_file_at(ROOT_INO, "/a", 0o644, 1).unwrap();
        assert_eq!(
            fs.create_file_at(ROOT_INO, "/a/b", 0o644, 1),
            Err(Errno::ENOTDIR)
        );
    }

    #[test]
    fn mkdir_updates_parent_nlink() {
        let mut fs = root();
        fs.mkdir_at(ROOT_INO, "/d", 0o755, 1).unwrap();
        assert_eq!(fs.stat(ROOT_INO).unwrap().nlink, 3);
        let ino = fs.lookup("/d").unwrap();
        assert_eq!(fs.stat(ino).unwrap().nlink, 2);
        assert_eq!(fs.audit(), Ok(()));
    }

    #[test]
    fn nested_directories_resolve() {
        let mut fs = root();
        fs.mkdir_at(ROOT_INO, "/a", 0o755, 1).unwrap();
        let a = fs.lookup("/a").unwrap();
        fs.mkdir_at(a, "b", 0o755, 1).unwrap();
        let b = fs.lookup("/a/b").unwrap();
        fs.create_file_at(b, "c", 0o644, 1).unwrap();
        assert!(fs.lookup("/a/b/c").is_ok());
        assert_eq!(fs.stat(a).unwrap().nlink, 3);
    }

    #[test]
    fn symlink_create_and_readlink() {
        let mut fs = root();
        fs.symlink_at(ROOT_INO, "/link", b"target", 1).unwrap();
        let ino = fs.lookup_no_follow("/link").unwrap();
        assert_eq!(fs.stat(ino).unwrap().mode & 0o170000, S_IFLNK);
        assert_eq!(fs.readlink_at(ROOT_INO, "/link").unwrap(), b"target");
    }

    #[test]
    fn lookup_follows_a_symlink() {
        let mut fs = root();
        let target = fs.create_file_at(ROOT_INO, "/target", 0o644, 1).unwrap();
        fs.symlink_at(ROOT_INO, "/link", b"target", 1).unwrap();
        assert_eq!(fs.lookup("/link").unwrap(), target);
    }

    #[test]
    fn lookup_no_follow_returns_the_symlink() {
        let mut fs = root();
        fs.create_file_at(ROOT_INO, "/target", 0o644, 1).unwrap();
        let link = fs.symlink_at(ROOT_INO, "/link", b"target", 1).unwrap();
        assert_eq!(fs.lookup_no_follow("/link").unwrap(), link);
        assert_ne!(fs.lookup("/link").unwrap(), link);
    }

    #[test]
    fn symlink_loop_is_eloop() {
        let mut fs = root();
        fs.symlink_at(ROOT_INO, "/loop", b"loop", 1).unwrap();
        assert_eq!(fs.lookup("/loop"), Err(Errno::ELOOP));
    }

    #[test]
    fn write_extends_with_zero_fill() {
        let mut fs = root();
        let ino = fs.create_file_at(ROOT_INO, "/f", 0o644, 1).unwrap();
        fs.write(ino, 4, b"xy", 2).unwrap();
        assert_eq!(read_all(&mut fs, ino), b"\0\0\0\0xy");
        assert_eq!(fs.total_bytes(), 6);
    }

    #[test]
    fn write_into_a_gap_keeps_zeros() {
        let mut fs = root();
        let ino = fs.create_file_at(ROOT_INO, "/f", 0o644, 1).unwrap();
        fs.write(ino, 0, b"abc", 2).unwrap();
        fs.write(ino, 8, b"z", 3).unwrap();
        assert_eq!(read_all(&mut fs, ino), b"abc\0\0\0\0\0z");
    }

    #[test]
    fn empty_write_does_not_extend() {
        let mut fs = root();
        let ino = fs.create_file_at(ROOT_INO, "/f", 0o644, 1).unwrap();
        fs.write(ino, 100, b"", 2).unwrap();
        assert_eq!(fs.len(ino).unwrap(), 0);
    }

    #[test]
    fn write_to_directory_is_eisdir() {
        let mut fs = root();
        let dir = fs.mkdir_at(ROOT_INO, "/d", 0o755, 1).unwrap();
        assert_eq!(fs.write(dir, 0, b"x", 2), Err(Errno::EISDIR));
    }

    #[test]
    fn write_to_symlink_is_einval() {
        let mut fs = root();
        let link = fs.symlink_at(ROOT_INO, "/l", b"x", 1).unwrap();
        assert_eq!(fs.write(link, 0, b"y", 2), Err(Errno::EINVAL));
    }

    #[test]
    fn read_eof_returns_zero() {
        let mut fs = root();
        let ino = fs.create_file_at(ROOT_INO, "/f", 0o644, 1).unwrap();
        fs.write(ino, 0, b"abc", 2).unwrap();
        let mut buffer = [0u8; 4];
        assert_eq!(fs.read(ino, 3, &mut buffer, 3), Ok(0));
    }

    #[test]
    fn read_updates_atime() {
        let mut fs = root();
        let ino = fs.create_file_at(ROOT_INO, "/f", 0o644, 1).unwrap();
        fs.write(ino, 0, b"abc", 2).unwrap();
        let mut buffer = [0u8; 2];
        fs.read(ino, 1, &mut buffer, 42).unwrap();
        assert_eq!(fs.stat(ino).unwrap().atime, 42);
        assert_eq!(&buffer, b"bc");
    }

    #[test]
    fn read_invalid_inode_is_ebadf() {
        let mut fs = root();
        let mut buffer = [0u8; 1];
        assert_eq!(fs.read(999, 0, &mut buffer, 1), Err(Errno::EBADF));
    }

    #[test]
    fn truncate_shrinks_and_grows() {
        let mut fs = root();
        let ino = fs.create_file_at(ROOT_INO, "/f", 0o644, 1).unwrap();
        fs.write(ino, 0, b"abcdef", 2).unwrap();
        fs.truncate(ino, 3, 3).unwrap();
        assert_eq!(read_all(&mut fs, ino), b"abc");
        assert_eq!(fs.total_bytes(), 3);
        fs.truncate(ino, 6, 4).unwrap();
        assert_eq!(read_all(&mut fs, ino), b"abc\0\0\0");
        assert_eq!(fs.total_bytes(), 6);
    }

    #[test]
    fn truncate_directory_is_eisdir() {
        let mut fs = root();
        let dir = fs.mkdir_at(ROOT_INO, "/d", 0o755, 1).unwrap();
        assert_eq!(fs.truncate(dir, 0, 2), Err(Errno::EISDIR));
    }

    #[test]
    fn unlink_keeps_open_inode_until_release() {
        let mut fs = root();
        let ino = fs.create_file_at(ROOT_INO, "/f", 0o644, 1).unwrap();
        fs.write(ino, 0, b"abcd", 2).unwrap();
        assert_eq!(fs.unlink_at(ROOT_INO, "/f", 3), Ok(ino));
        assert_eq!(fs.lookup("/f"), Err(Errno::ENOENT));
        assert_eq!(fs.stat(ino).unwrap().nlink, 0);
        assert_eq!(fs.total_bytes(), 4);
        assert_eq!(fs.release(ino), Ok(()));
        assert_eq!(fs.total_bytes(), 0);
        assert_eq!(fs.stat(ino), Err(Errno::EBADF));
    }

    #[test]
    fn unlink_directory_is_eisdir() {
        let mut fs = root();
        fs.mkdir_at(ROOT_INO, "/d", 0o755, 1).unwrap();
        assert_eq!(fs.unlink_at(ROOT_INO, "/d", 2), Err(Errno::EISDIR));
    }

    #[test]
    fn unlink_missing_is_enoent() {
        let mut fs = root();
        assert_eq!(fs.unlink_at(ROOT_INO, "/none", 2), Err(Errno::ENOENT));
    }

    #[test]
    fn rmdir_empty_directory() {
        let mut fs = root();
        let ino = fs.mkdir_at(ROOT_INO, "/d", 0o755, 1).unwrap();
        assert_eq!(fs.rmdir_at(ROOT_INO, "/d", 2), Ok(ino));
        assert_eq!(fs.stat(ROOT_INO).unwrap().nlink, 2);
        assert!(fs.lookup("/d").is_err());
        assert_eq!(fs.release(ino), Ok(()));
    }

    #[test]
    fn rmdir_nonempty_is_enotempty() {
        let mut fs = root();
        fs.mkdir_at(ROOT_INO, "/d", 0o755, 1).unwrap();
        fs.create_file_at(fs.lookup("/d").unwrap(), "f", 0o644, 2)
            .unwrap();
        assert_eq!(fs.rmdir_at(ROOT_INO, "/d", 3), Err(Errno::ENOTEMPTY));
    }

    #[test]
    fn rmdir_file_is_enotdir() {
        let mut fs = root();
        fs.create_file_at(ROOT_INO, "/f", 0o644, 1).unwrap();
        assert_eq!(fs.rmdir_at(ROOT_INO, "/f", 2), Err(Errno::ENOTDIR));
    }

    #[test]
    fn rmdir_root_is_einval() {
        let mut fs = root();
        assert_eq!(fs.rmdir_at(ROOT_INO, "/", 2), Err(Errno::EINVAL));
    }

    #[test]
    fn rename_file() {
        let mut fs = root();
        let ino = fs.create_file_at(ROOT_INO, "/a", 0o644, 1).unwrap();
        assert_eq!(fs.rename_at(ROOT_INO, "/a", "/b", 2), Ok(None));
        assert_eq!(fs.lookup("/b").unwrap(), ino);
        assert_eq!(fs.lookup("/a"), Err(Errno::ENOENT));
    }

    #[test]
    fn rename_replaces_a_file() {
        let mut fs = root();
        let source = fs.create_file_at(ROOT_INO, "/a", 0o644, 1).unwrap();
        let replaced = fs.create_file_at(ROOT_INO, "/b", 0o644, 1).unwrap();
        assert_eq!(fs.rename_at(ROOT_INO, "/a", "/b", 2), Ok(Some(replaced)));
        assert_eq!(fs.lookup("/b").unwrap(), source);
        assert_eq!(fs.stat(replaced).unwrap().nlink, 0);
        assert_eq!(fs.release(replaced), Ok(()));
    }

    #[test]
    fn rename_same_path_is_a_no_op() {
        let mut fs = root();
        fs.create_file_at(ROOT_INO, "/a", 0o644, 1).unwrap();
        assert_eq!(fs.rename_at(ROOT_INO, "/a", "/a", 2), Ok(None));
        assert!(fs.lookup("/a").is_ok());
    }

    #[test]
    fn rename_file_over_directory_is_eisdir() {
        let mut fs = root();
        fs.create_file_at(ROOT_INO, "/a", 0o644, 1).unwrap();
        fs.mkdir_at(ROOT_INO, "/d", 0o755, 1).unwrap();
        assert_eq!(fs.rename_at(ROOT_INO, "/a", "/d", 2), Err(Errno::EISDIR));
    }

    #[test]
    fn rename_directory_over_file_is_enotdir() {
        let mut fs = root();
        fs.mkdir_at(ROOT_INO, "/d", 0o755, 1).unwrap();
        fs.create_file_at(ROOT_INO, "/f", 0o644, 1).unwrap();
        assert_eq!(fs.rename_at(ROOT_INO, "/d", "/f", 2), Err(Errno::ENOTDIR));
    }

    #[test]
    fn rename_directory_into_itself_is_einval() {
        let mut fs = root();
        let d = fs.mkdir_at(ROOT_INO, "/d", 0o755, 1).unwrap();
        fs.mkdir_at(d, "e", 0o755, 1).unwrap();
        assert_eq!(
            fs.rename_at(ROOT_INO, "/d", "/d/e/x", 2),
            Err(Errno::EINVAL)
        );
    }

    #[test]
    fn rename_directory_over_nonempty_is_enotempty() {
        let mut fs = root();
        fs.mkdir_at(ROOT_INO, "/a", 0o755, 1).unwrap();
        let b = fs.mkdir_at(ROOT_INO, "/b", 0o755, 1).unwrap();
        fs.create_file_at(b, "f", 0o644, 1).unwrap();
        assert_eq!(fs.rename_at(ROOT_INO, "/a", "/b", 2), Err(Errno::ENOTEMPTY));
    }

    #[test]
    fn rename_directory_across_parents_updates_nlink() {
        let mut fs = root();
        let a = fs.mkdir_at(ROOT_INO, "/a", 0o755, 1).unwrap();
        let b = fs.mkdir_at(ROOT_INO, "/b", 0o755, 1).unwrap();
        assert_eq!(fs.rename_at(ROOT_INO, "/a", "/b/c", 2), Ok(None));
        assert_eq!(fs.stat(a).unwrap().nlink, 2);
        assert_eq!(fs.stat(b).unwrap().nlink, 3);
        assert_eq!(fs.audit(), Ok(()));
    }

    #[test]
    fn rename_directory_over_empty_directory_updates_nlink() {
        let mut fs = root();
        let a = fs.mkdir_at(ROOT_INO, "/a", 0o755, 1).unwrap();
        let b = fs.mkdir_at(ROOT_INO, "/b", 0o755, 1).unwrap();
        assert_eq!(fs.rename_at(ROOT_INO, "/a", "/b", 2), Ok(Some(b)));
        assert_eq!(fs.stat(ROOT_INO).unwrap().nlink, 3);
        assert_eq!(fs.lookup("/b").unwrap(), a);
        assert_eq!(fs.release(b), Ok(()));
        assert_eq!(fs.audit(), Ok(()));
    }

    #[test]
    fn rename_missing_source_is_enoent() {
        let mut fs = root();
        assert_eq!(
            fs.rename_at(ROOT_INO, "/missing", "/x", 2),
            Err(Errno::ENOENT)
        );
    }

    #[test]
    fn rename_destination_parent_file_is_enotdir() {
        let mut fs = root();
        fs.create_file_at(ROOT_INO, "/a", 0o644, 1).unwrap();
        fs.create_file_at(ROOT_INO, "/f", 0o644, 1).unwrap();
        assert_eq!(fs.rename_at(ROOT_INO, "/a", "/f/b", 2), Err(Errno::ENOTDIR));
    }

    #[test]
    fn chmod_changes_mode_and_ctime() {
        let mut fs = root();
        let ino = fs.create_file_at(ROOT_INO, "/f", 0o644, 1).unwrap();
        fs.chmod_at(ROOT_INO, "/f", 0o600, 7).unwrap();
        let stat = fs.stat(ino).unwrap();
        assert_eq!(stat.mode, S_IFREG | 0o600);
        assert_eq!(stat.ctime, 7);
    }

    #[test]
    fn statfs_reports_limits_and_usage() {
        let mut fs = small();
        let ino = fs.create_file_at(ROOT_INO, "/f", 0o644, 1).unwrap();
        fs.write(ino, 0, &[1u8; 100], 2).unwrap();
        let statfs = fs.statfs();
        assert_eq!(statfs.block_size, BLOCK_SIZE);
        assert_eq!(statfs.blocks, 1);
        assert_eq!(statfs.files, 8);
        assert_eq!(statfs.free_files, 6);
        assert_eq!(statfs.free_blocks, 0);
    }

    #[test]
    fn readdir_returns_sorted_names() {
        let mut fs = root();
        fs.create_file_at(ROOT_INO, "/b", 0o644, 1).unwrap();
        fs.mkdir_at(ROOT_INO, "/a", 0o755, 1).unwrap();
        fs.symlink_at(ROOT_INO, "/c", b"a", 1).unwrap();
        assert_eq!(names(&mut fs, ROOT_INO), ["a", "b", "c"]);
    }

    #[test]
    fn readdir_cookie_continues() {
        let mut fs = root();
        fs.create_file_at(ROOT_INO, "/a", 0o644, 1).unwrap();
        fs.create_file_at(ROOT_INO, "/b", 0o644, 1).unwrap();
        let mut first = [0u8; 24];
        let length = fs.readdir(ROOT_INO, 0, &mut first, 1).unwrap();
        assert_eq!(length, 24);
        let mut second = [0u8; 24];
        let length = fs.readdir(ROOT_INO, 24, &mut second, 1).unwrap();
        assert_eq!(length, 24);
        let first_name = DirEntryIter::new(&first).next().unwrap().unwrap();
        let second_name = DirEntryIter::new(&second).next().unwrap().unwrap();
        assert_eq!(first_name.name, b"a");
        assert_eq!(second_name.name, b"b");
    }

    #[test]
    fn readdir_mid_record_cookie_is_einval() {
        let mut fs = root();
        fs.create_file_at(ROOT_INO, "/a", 0o644, 1).unwrap();
        let mut out = [0u8; 64];
        assert_eq!(fs.readdir(ROOT_INO, 5, &mut out, 1), Err(Errno::EINVAL));
    }

    #[test]
    fn readdir_small_buffer_is_einval() {
        let mut fs = root();
        fs.create_file_at(ROOT_INO, "/a", 0o644, 1).unwrap();
        let mut out = [0u8; 8];
        assert_eq!(fs.readdir(ROOT_INO, 0, &mut out, 1), Err(Errno::EINVAL));
    }

    #[test]
    fn readdir_on_file_is_enotdir() {
        let mut fs = root();
        let ino = fs.create_file_at(ROOT_INO, "/f", 0o644, 1).unwrap();
        let mut out = [0u8; 64];
        assert_eq!(fs.readdir(ino, 0, &mut out, 1), Err(Errno::ENOTDIR));
    }

    #[test]
    fn readdir_end_cookie_returns_zero() {
        let mut fs = root();
        fs.create_file_at(ROOT_INO, "/a", 0o644, 1).unwrap();
        let mut out = [0u8; 64];
        assert_eq!(fs.readdir(ROOT_INO, 24, &mut out, 1), Ok(0));
    }

    #[test]
    fn name_too_long_is_enametoolong() {
        let mut fs = small();
        assert_eq!(
            fs.create_file_at(ROOT_INO, "/123456789", 0o644, 1),
            Err(Errno::ENAMETOOLONG)
        );
    }

    #[test]
    fn depth_limit_is_enomem() {
        let mut fs = small();
        assert_eq!(
            fs.mkdir_at(ROOT_INO, "/a/b/c/d/e", 0o755, 1),
            Err(Errno::ENOMEM)
        );
    }

    #[test]
    fn dot_components_are_einval() {
        let mut fs = root();
        assert_eq!(
            fs.create_file_at(ROOT_INO, "/a/../b", 0o644, 1),
            Err(Errno::EINVAL)
        );
        assert_eq!(fs.lookup("/a/./b"), Err(Errno::EINVAL));
    }

    #[test]
    fn nul_path_is_einval() {
        let mut fs = root();
        assert_eq!(
            fs.create_file_at(ROOT_INO, "/a\0b", 0o644, 1),
            Err(Errno::EINVAL)
        );
    }

    #[test]
    fn byte_limit_write_is_enospc() {
        let mut fs = small();
        let ino = fs.create_file_at(ROOT_INO, "/f", 0o644, 1).unwrap();
        assert_eq!(fs.write(ino, 0, &vec![1u8; 4097], 2), Err(Errno::ENOSPC));
        assert_eq!(fs.total_bytes(), 0);
        fs.write(ino, 0, &vec![1u8; 4096], 2).unwrap();
        assert_eq!(fs.write(ino, 4096, b"x", 3), Err(Errno::ENOSPC));
    }

    #[test]
    fn byte_limit_truncate_is_enospc() {
        let mut fs = small();
        let ino = fs.create_file_at(ROOT_INO, "/f", 0o644, 1).unwrap();
        assert_eq!(fs.truncate(ino, 4097, 2), Err(Errno::ENOSPC));
    }

    #[test]
    fn inode_limit_is_enospc() {
        let mut fs = small();
        for index in 0..7 {
            fs.create_file_at(ROOT_INO, &format!("/f{index}"), 0o644, 1)
                .unwrap();
        }
        assert_eq!(
            fs.create_file_at(ROOT_INO, "/last", 0o644, 1),
            Err(Errno::ENOSPC)
        );
    }

    #[test]
    fn hard_links_are_eperm() {
        let mut fs = root();
        fs.create_file_at(ROOT_INO, "/f", 0o644, 1).unwrap();
        assert_eq!(fs.link_at(ROOT_INO, "/g", "/f", 2), Err(Errno::EPERM));
    }

    #[test]
    fn release_linked_inode_is_einval() {
        let mut fs = root();
        let ino = fs.create_file_at(ROOT_INO, "/f", 0o644, 1).unwrap();
        assert_eq!(fs.release(ino), Err(Errno::EINVAL));
    }

    #[test]
    fn stat_size_and_blocks() {
        let mut fs = root();
        let ino = fs.create_file_at(ROOT_INO, "/f", 0o644, 1).unwrap();
        fs.write(ino, 0, &vec![1u8; 5000], 2).unwrap();
        let stat = fs.stat(ino).unwrap();
        assert_eq!(stat.size, 5000);
        assert_eq!(stat.blocks, 10);
    }

    #[test]
    fn create_mode_is_preserved() {
        let mut fs = root();
        let ino = fs.create_file_at(ROOT_INO, "/f", 0o640, 1).unwrap();
        assert_eq!(fs.stat(ino).unwrap().mode, S_IFREG | 0o640);
        let dir = fs.mkdir_at(ROOT_INO, "/d", 0o750, 1).unwrap();
        assert_eq!(fs.stat(dir).unwrap().mode, S_IFDIR | 0o750);
    }

    #[test]
    fn relative_lookup_uses_the_base() {
        let mut fs = root();
        let d = fs.mkdir_at(ROOT_INO, "/d", 0o755, 1).unwrap();
        let f = fs.create_file_at(d, "f", 0o644, 1).unwrap();
        assert_eq!(fs.lookup_at(d, "f").unwrap(), f);
        assert_eq!(fs.lookup("/d/f").unwrap(), f);
    }

    #[test]
    fn absolute_lookup_ignores_the_base() {
        let mut fs = root();
        let d = fs.mkdir_at(ROOT_INO, "/d", 0o755, 1).unwrap();
        let f = fs.create_file_at(ROOT_INO, "/f", 0o644, 1).unwrap();
        assert_eq!(fs.lookup_at(d, "/f").unwrap(), f);
    }

    #[test]
    fn readdir_updates_directory_atime() {
        let mut fs = root();
        fs.mkdir_at(ROOT_INO, "/d", 0o755, 1).unwrap();
        let d = fs.lookup("/d").unwrap();
        let mut out = [0u8; 64];
        fs.readdir(d, 0, &mut out, 99).unwrap();
        assert_eq!(fs.stat(d).unwrap().atime, 99);
    }

    #[test]
    fn rename_updates_parent_times() {
        let mut fs = root();
        fs.create_file_at(ROOT_INO, "/a", 0o644, 1).unwrap();
        fs.rename_at(ROOT_INO, "/a", "/b", 55).unwrap();
        let stat = fs.stat(ROOT_INO).unwrap();
        assert_eq!(stat.mtime, 55);
        assert_eq!(stat.ctime, 55);
    }

    #[test]
    fn audit_accepts_a_populated_tree() {
        let mut fs = root();
        let d = fs.mkdir_at(ROOT_INO, "/d", 0o755, 1).unwrap();
        fs.create_file_at(d, "f", 0o644, 1).unwrap();
        fs.symlink_at(d, "l", b"f", 1).unwrap();
        assert_eq!(fs.audit(), Ok(()));
        assert_eq!(fs.node_count(), 4);
    }

    #[test]
    fn lookup_empty_path_returns_the_base() {
        let mut fs = root();
        let d = fs.mkdir_at(ROOT_INO, "/d", 0o755, 1).unwrap();
        assert_eq!(fs.lookup_at(d, "").unwrap(), d);
        assert_eq!(fs.lookup_at(d, "/").unwrap(), ROOT_INO);
    }

    #[test]
    fn readlink_on_file_is_einval() {
        let mut fs = root();
        fs.create_file_at(ROOT_INO, "/f", 0o644, 1).unwrap();
        assert_eq!(fs.readlink_at(ROOT_INO, "/f"), Err(Errno::EINVAL));
    }

    #[derive(Clone, Debug)]
    enum ModelEntry {
        File(Vec<u8>),
        Dir,
        Link,
    }

    fn next_random(state: &mut u64) -> u64 {
        let mut value = *state;
        value ^= value << 13;
        value ^= value >> 7;
        value ^= value << 17;
        *state = value;
        value
    }

    fn model_bytes(model: &std::collections::HashMap<String, ModelEntry>) -> usize {
        model
            .values()
            .map(|entry| match entry {
                ModelEntry::File(data) => data.len(),
                _ => 0,
            })
            .sum()
    }

    fn model_check(fs: &Ramfs, model: &std::collections::HashMap<String, ModelEntry>) {
        fs.audit().unwrap();
        assert_eq!(fs.node_count(), model.len() + 1);
        let expected = model_bytes(model);
        if fs.total_bytes() != expected {
            panic!(
                "total mismatch: core={} model={} entries={model:?}",
                fs.total_bytes(),
                expected
            );
        }
    }

    fn model_seed(seed: u64) {
        let mut fs = Ramfs::with_limits(
            Limits {
                max_bytes: 4096,
                max_inodes: 16,
                max_name: 8,
                max_depth: 4,
            },
            0,
        );
        let mut model: std::collections::HashMap<String, ModelEntry> =
            std::collections::HashMap::new();
        let mut random = seed;
        let mut now = 1i64;
        for _ in 0..20_000 {
            let op = next_random(&mut random) % 9;
            let name = format!("/{}", (b'a' + (next_random(&mut random) % 8) as u8) as char);
            let dest = format!("/{}", (b'a' + (next_random(&mut random) % 8) as u8) as char);
            match op {
                0 => {
                    let expected = if model.len() + 1 >= 16 {
                        Err(Errno::ENOSPC)
                    } else if model.contains_key(&name) {
                        Err(Errno::EEXIST)
                    } else {
                        model.insert(name.clone(), ModelEntry::File(Vec::new()));
                        Ok(())
                    };
                    let result = fs.create_file_at(ROOT_INO, &name, 0o644, now).map(|_| ());
                    assert_eq!(result, expected, "create {name} op {op}");
                }
                1 => {
                    let expected = if model.len() + 1 >= 16 {
                        Err(Errno::ENOSPC)
                    } else if model.contains_key(&name) {
                        Err(Errno::EEXIST)
                    } else {
                        model.insert(name.clone(), ModelEntry::Dir);
                        Ok(())
                    };
                    let result = fs.mkdir_at(ROOT_INO, &name, 0o755, now).map(|_| ());
                    assert_eq!(result, expected, "mkdir {name}");
                }
                2 => {
                    let expected = if model.len() + 1 >= 16 {
                        Err(Errno::ENOSPC)
                    } else if model.contains_key(&name) {
                        Err(Errno::EEXIST)
                    } else {
                        model.insert(name.clone(), ModelEntry::Link);
                        Ok(())
                    };
                    let result = fs.symlink_at(ROOT_INO, &name, b"a", now).map(|_| ());
                    assert_eq!(result, expected, "symlink {name}");
                }
                3 => {
                    if let Some(ModelEntry::File(data)) = model.get(&name) {
                        let offset = (next_random(&mut random) % 64) as usize;
                        let length = (next_random(&mut random) % 32) as usize;
                        let new_len = if length == 0 {
                            data.len()
                        } else {
                            data.len().max(offset.saturating_add(length))
                        };
                        let growth = new_len - data.len();
                        let expected = if growth > 0 && model_bytes(&model) + growth > 4096 {
                            Err(Errno::ENOSPC)
                        } else {
                            Ok(length)
                        };
                        let ino = fs.lookup(&name).unwrap();
                        let result = fs.write(ino, offset as u64, &vec![0x5a; length], now);
                        assert_eq!(result, expected, "write {name}");
                        if result.is_ok()
                            && let Some(ModelEntry::File(data)) = model.get_mut(&name)
                        {
                            if new_len > data.len() {
                                data.resize(new_len, 0);
                            }
                            if length > 0 {
                                data[offset..offset + length].fill(0x5a);
                            }
                        }
                    }
                }
                4 => {
                    if let Some(ModelEntry::File(data)) = model.get(&name) {
                        let size = (next_random(&mut random) % 2048) as usize;
                        let growth = size.saturating_sub(data.len());
                        let expected = if growth > 0 && model_bytes(&model) + growth > 4096 {
                            Err(Errno::ENOSPC)
                        } else {
                            Ok(())
                        };
                        let ino = fs.lookup(&name).unwrap();
                        let result = fs.truncate(ino, size as u64, now);
                        assert_eq!(result, expected, "truncate {name}");
                        if result.is_ok()
                            && let Some(ModelEntry::File(data)) = model.get_mut(&name)
                        {
                            data.resize(size, 0);
                        }
                    }
                }
                5 => {
                    if let Some(ModelEntry::File(data)) = model.get(&name) {
                        let offset = (next_random(&mut random) % 72) as usize;
                        let mut buffer = vec![0u8; 32];
                        let ino = fs.lookup(&name).unwrap();
                        let length = fs.read(ino, offset as u64, &mut buffer, now).unwrap();
                        let expected_data = data.get(offset..).unwrap_or(&[]);
                        let expected_len = expected_data.len().min(32);
                        assert_eq!(length, expected_len, "read length {name}");
                        assert_eq!(&buffer[..length], &expected_data[..expected_len]);
                    }
                }
                6 => {
                    let expected = match model.get(&name) {
                        None => Err(Errno::ENOENT),
                        Some(ModelEntry::Dir) => Err(Errno::EISDIR),
                        Some(_) => Ok(()),
                    };
                    match expected {
                        Ok(()) => {
                            let ino = fs.unlink_at(ROOT_INO, &name, now).unwrap();
                            fs.release(ino).unwrap();
                            model.remove(&name);
                        }
                        Err(error) => {
                            assert_eq!(
                                fs.unlink_at(ROOT_INO, &name, now),
                                Err(error),
                                "unlink {name}"
                            );
                        }
                    }
                }
                7 => {
                    let expected = match model.get(&name) {
                        None => Err(Errno::ENOENT),
                        Some(ModelEntry::Dir) => Ok(()),
                        Some(_) => Err(Errno::ENOTDIR),
                    };
                    match expected {
                        Ok(()) => {
                            let ino = fs.rmdir_at(ROOT_INO, &name, now).unwrap();
                            fs.release(ino).unwrap();
                            model.remove(&name);
                        }
                        Err(error) => {
                            assert_eq!(
                                fs.rmdir_at(ROOT_INO, &name, now),
                                Err(error),
                                "rmdir {name}"
                            );
                        }
                    }
                }
                _ => {
                    if name == dest {
                        continue;
                    }
                    let source = model.get(&name).cloned();
                    let dest_entry = model.get(&dest).cloned();
                    let expected = match (&source, &dest_entry) {
                        (None, _) => Err(Errno::ENOENT),
                        (_, Some(ModelEntry::Dir)) if !matches!(source, Some(ModelEntry::Dir)) => {
                            Err(Errno::EISDIR)
                        }
                        (Some(ModelEntry::Dir), Some(entry))
                            if !matches!(entry, ModelEntry::Dir) =>
                        {
                            Err(Errno::ENOTDIR)
                        }
                        _ => Ok(()),
                    };
                    match expected {
                        Ok(()) => {
                            let replaced = fs.rename_at(ROOT_INO, &name, &dest, now).unwrap();
                            if let Some(ino) = replaced {
                                fs.release(ino).unwrap();
                            }
                            let entry = model.remove(&name).unwrap();
                            model.insert(dest.clone(), entry);
                        }
                        Err(error) => {
                            assert_eq!(
                                fs.rename_at(ROOT_INO, &name, &dest, now),
                                Err(error),
                                "rename {name} {dest}"
                            );
                        }
                    }
                }
            }
            model_check(&fs, &model);
            now += 1;
        }
    }

    #[test]
    fn randomized_model_seed_one() {
        model_seed(0x1234_5678_9abc_def0);
    }

    #[test]
    fn randomized_model_seed_two() {
        model_seed(0x0f0e_0d0c_0b0a_0908);
    }
}
