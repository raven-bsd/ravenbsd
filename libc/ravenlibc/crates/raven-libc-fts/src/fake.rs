// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//!
//! The std-free fake filesystem of doc 07 4.5 (feature `testutil`).
//!
//! Nodes are directories, regular files, symbolic links or typed nodes (fifo, char, block,
//! socket). Each has a stable `st_dev`/`st_ino`, an owner permission mask, a hard-link count and
//! a counter `mtime` (never a clock). `getdents` emits `.`, `..` and the children in creation
//! order; each record's `d_off` is the cookie that resumes after it (OI-2). Descriptors are small
//! integers allocated lowest-free; `emfile_after(n)` fails the `(n+1)`-th simultaneous open with
//! `EMFILE`. `fail_after` and `hook_after` inject a one-shot error or a tree mutation between two
//! calls, and every trait call is logged.
//!
//! No real filesystem is touched: this module is `alloc` only and uses no `std` path.

use alloc::boxed::Box;
use alloc::vec::Vec;

use raven_libc_abi::errno::Errno;
use raven_libc_abi::generated::errno::{EACCES, EBADF, EINVAL, ELOOP, EMFILE, ENOENT, ENOTDIR};
use raven_libc_abi::generated::fcntl::{O_DIRECTORY, O_NOFOLLOW, O_PATH};
use raven_libc_abi::generated::stat::{
    DT_BLK, DT_CHR, DT_DIR, DT_FIFO, DT_LNK, DT_REG, DT_SOCK, S_IFBLK, S_IFCHR, S_IFDIR, S_IFIFO,
    S_IFLNK, S_IFMT, S_IFREG, S_IFSOCK, S_IRUSR, S_IXUSR,
};
use raven_libc_abi::generated::structs::{Stat, Timespec};

use crate::consts::SYMLOOP_MAX;
use crate::{AT_FDCWD, DirSys, Fd, Whence};

/// The type of a fake node.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum NodeKind {
    /// A directory.
    Dir,
    /// A regular file.
    File,
    /// A symbolic link (`target` bytes).
    Symlink,
    /// A FIFO.
    Fifo,
    /// A character device.
    Char,
    /// A block device.
    Block,
    /// A socket.
    Socket,
}

fn type_mode(kind: NodeKind) -> u32 {
    match kind {
        NodeKind::Dir => S_IFDIR,
        NodeKind::File => S_IFREG,
        NodeKind::Symlink => S_IFLNK,
        NodeKind::Fifo => S_IFIFO,
        NodeKind::Char => S_IFCHR,
        NodeKind::Block => S_IFBLK,
        NodeKind::Socket => S_IFSOCK,
    }
}

fn dirent_type(kind: NodeKind) -> u8 {
    match kind {
        NodeKind::Dir => DT_DIR as u8,
        NodeKind::File => DT_REG as u8,
        NodeKind::Symlink => DT_LNK as u8,
        NodeKind::Fifo => DT_FIFO as u8,
        NodeKind::Char => DT_CHR as u8,
        NodeKind::Block => DT_BLK as u8,
        NodeKind::Socket => DT_SOCK as u8,
    }
}

struct FakeNode {
    kind: NodeKind,
    mode: u32,
    dev: u64,
    ino: u64,
    nlink: u32,
    size: i64,
    mtime: i64,
    parent: Option<usize>,
    children: Vec<(Vec<u8>, usize)>,
    target: Vec<u8>,
}

#[derive(Clone)]
struct OpenFd {
    node: usize,
    offset: i64,
    is_path: bool,
}

/// One logged trait call.
#[derive(Clone, Debug)]
pub struct Call {
    /// The method name.
    pub method: &'static str,
    /// The directory descriptor argument (`AT_FDCWD` for the path based entry points).
    pub dirfd: i32,
    /// The name argument.
    pub name: Vec<u8>,
    /// The `openat` flags, or 0.
    pub flags: u32,
}

/// A test hook run between two fake calls.
type Hook = Box<dyn FnMut(&mut FakeFs)>;

/// The fake filesystem.
pub struct FakeFs {
    nodes: Vec<FakeNode>,
    root: usize,
    cwd: usize,
    fds: Vec<Option<OpenFd>>,
    next_ino: u64,
    mtime: i64,
    ops: usize,
    fail: Option<(usize, Errno)>,
    hook: Option<(usize, Hook)>,
    dirent_limit: Option<usize>,
    emfile: Option<usize>,
    /// Every call, in order.
    pub log: Vec<Call>,
}

impl FakeFs {
    /// A filesystem with a writable root and `cwd = /`.
    #[must_use]
    pub fn new() -> Self {
        let root = FakeNode {
            kind: NodeKind::Dir,
            mode: S_IFDIR | 0o755,
            dev: 1,
            ino: 1,
            nlink: 2,
            size: 0,
            mtime: 0,
            parent: None,
            children: Vec::new(),
            target: Vec::new(),
        };
        Self {
            nodes: alloc::vec![root],
            root: 0,
            cwd: 0,
            fds: Vec::new(),
            next_ino: 2,
            mtime: 0,
            ops: 0,
            fail: None,
            hook: None,
            dirent_limit: None,
            emfile: None,
            log: Vec::new(),
        }
    }

    fn alloc(&mut self, kind: NodeKind, parent: usize, name: &[u8], mode: u32) -> usize {
        self.next_ino = self.next_ino.wrapping_add(1);
        self.mtime = self.mtime.wrapping_add(1);
        let ino = self.next_ino;
        let dev = self.nodes.get(parent).map_or(1, |node| node.dev);
        let id = self.nodes.len();
        self.nodes.push(FakeNode {
            kind,
            mode: type_mode(kind) | mode,
            dev,
            ino,
            nlink: if kind == NodeKind::Dir { 2 } else { 1 },
            size: 0,
            mtime: self.mtime,
            parent: Some(parent),
            children: Vec::new(),
            target: Vec::new(),
        });
        if let Some(node) = self.nodes.get_mut(parent) {
            node.children.push((name.to_vec(), id));
            if kind == NodeKind::Dir {
                node.nlink = node.nlink.saturating_add(1);
            }
        }
        id
    }

    fn split_parent(&self, path: &[u8]) -> (usize, Vec<u8>) {
        let path = path.strip_prefix(b"/").unwrap_or(path);
        let split = path
            .iter()
            .rposition(|&b| b == b'/')
            .map_or(0, |index| index.wrapping_add(1));
        let parent_path = path.get(..split.saturating_sub(1)).unwrap_or(&[]);
        let name = path.get(split..).unwrap_or(&[]).to_vec();
        let parent = self.resolve_path(parent_path, true).unwrap_or(self.cwd);
        (parent, name)
    }

    /// Create a directory at `path`.
    pub fn mkdir(&mut self, path: impl AsRef<[u8]>, mode: u32) {
        let (parent, name) = self.split_parent(path.as_ref());
        self.alloc(NodeKind::Dir, parent, &name, mode);
    }

    /// Create a regular file of `size` bytes.
    pub fn file(&mut self, path: impl AsRef<[u8]>, size: i64) {
        let (parent, name) = self.split_parent(path.as_ref());
        let id = self.alloc(NodeKind::File, parent, &name, 0o644);
        if let Some(node) = self.nodes.get_mut(id) {
            node.size = size;
        }
    }

    /// Create a typed node (`Fifo`, `Char`, `Block`, `Socket`).
    pub fn node(&mut self, path: impl AsRef<[u8]>, kind: NodeKind) {
        let (parent, name) = self.split_parent(path.as_ref());
        self.alloc(kind, parent, &name, 0o644);
    }

    /// Create a symbolic link at `path` with `target` bytes.
    pub fn symlink(&mut self, path: impl AsRef<[u8]>, target: impl AsRef<[u8]>) {
        let (parent, name) = self.split_parent(path.as_ref());
        let id = self.alloc(NodeKind::Symlink, parent, &name, 0o777);
        if let Some(node) = self.nodes.get_mut(id) {
            node.target = target.as_ref().to_vec();
        }
    }

    /// Change the permission bits of `path` (the type bits stay).
    pub fn chmod(&mut self, path: impl AsRef<[u8]>, mode: u32) {
        if let Ok(id) = self.resolve_path(path.as_ref(), true)
            && let Some(node) = self.nodes.get_mut(id)
        {
            node.mode = (node.mode & S_IFMT) | (mode & !S_IFMT);
        }
    }

    /// Remove `path` from its parent (the node itself stays in the arena).
    pub fn remove(&mut self, path: impl AsRef<[u8]>) {
        let (parent, name) = self.split_parent(path.as_ref());
        if let Some(node) = self.nodes.get_mut(parent)
            && let Some(index) = node.children.iter().position(|(n, _)| n == &name)
        {
            node.children.remove(index);
        }
    }

    /// Rename `old` to `new` (same node, same inode).
    pub fn rename(&mut self, old: impl AsRef<[u8]>, new: impl AsRef<[u8]>) {
        let (old_parent, old_name) = self.split_parent(old.as_ref());
        let id = match self.nodes.get(old_parent).and_then(|node| {
            node.children
                .iter()
                .find(|(n, _)| n == &old_name)
                .map(|&(_, id)| id)
        }) {
            Some(id) => id,
            None => return,
        };
        self.remove(old.as_ref());
        let (new_parent, new_name) = self.split_parent(new.as_ref());
        if let Some(node) = self.nodes.get_mut(new_parent) {
            node.children.push((new_name, id));
        }
        if let Some(node) = self.nodes.get_mut(id) {
            node.parent = Some(new_parent);
        }
    }

    /// Add a hard link `path` to `existing`.
    pub fn link(&mut self, path: impl AsRef<[u8]>, existing: impl AsRef<[u8]>) {
        let Ok(id) = self.resolve_path(existing.as_ref(), false) else {
            return;
        };
        let (parent, name) = self.split_parent(path.as_ref());
        if let Some(node) = self.nodes.get_mut(parent) {
            node.children.push((name, id));
        }
        if let Some(node) = self.nodes.get_mut(id) {
            node.nlink = node.nlink.saturating_add(1);
        }
    }

    /// Make the subtree under `path` report `st_dev = dev`.
    pub fn mount(&mut self, path: impl AsRef<[u8]>, dev: u64) {
        if let Ok(id) = self.resolve_path(path.as_ref(), true) {
            self.set_dev(id, dev);
        }
    }

    fn set_dev(&mut self, id: usize, dev: u64) {
        let children: Vec<usize> = match self.nodes.get_mut(id) {
            Some(node) => {
                node.dev = dev;
                node.children.iter().map(|&(_, child)| child).collect()
            }
            None => return,
        };
        for child in children {
            self.set_dev(child, dev);
        }
    }

    /// Set the fake `cwd` used by relative paths and `AT_FDCWD`.
    pub fn set_cwd(&mut self, path: impl AsRef<[u8]>) {
        if let Ok(id) = self.resolve_path(path.as_ref(), true) {
            self.cwd = id;
        }
    }

    /// The number of entries of a directory including `.` and `..`.
    #[must_use]
    pub fn entry_count(&self, path: &[u8]) -> usize {
        match self.resolve_path(path, true) {
            Ok(id) => match self.nodes.get(id) {
                Some(node) => node.children.len().wrapping_add(2),
                None => 0,
            },
            Err(_) => 0,
        }
    }

    /// The number of trait calls so far.
    #[must_use]
    pub fn op_count(&self) -> usize {
        self.ops
    }

    /// The number of simultaneously open descriptors.
    #[must_use]
    pub fn open_fd_count(&self) -> usize {
        self.fds.iter().filter(|slot| slot.is_some()).count()
    }

    /// Make the `after`-th trait call fail once with `errno`.
    pub fn fail_after(&mut self, after: usize, errno: i32) {
        self.fail = Some((after, Errno(errno as u16)));
    }

    /// Run `hook` after the `after`-th trait call; it may mutate the tree.
    pub fn hook_after(&mut self, after: usize, hook: Hook) {
        self.hook = Some((after, hook));
    }

    /// Cap the bytes `getdents` returns per call.
    pub fn set_dirent_buf_limit(&mut self, limit: usize) {
        self.dirent_limit = Some(limit);
    }

    /// Make the `(n+1)`-th simultaneously open descriptor fail with `EMFILE`.
    pub fn emfile_after(&mut self, n: usize) {
        self.emfile = Some(n);
    }

    fn child_id(&self, dir: usize, name: &[u8]) -> Option<usize> {
        if name == b"." {
            return Some(dir);
        }
        if name == b".." {
            return Some(match self.nodes.get(dir) {
                Some(node) => node.parent.unwrap_or(self.root),
                None => self.root,
            });
        }
        self.nodes
            .get(dir)
            .and_then(|node| node.children.iter().find(|(n, _)| n == name))
            .map(|&(_, id)| id)
    }

    fn resolve_path(&self, path: &[u8], follow_last: bool) -> Result<usize, Errno> {
        let (start, rest) = if path.first() == Some(&b'/') {
            (self.root, path.strip_prefix(b"/").unwrap_or(path))
        } else {
            (self.cwd, path)
        };
        self.walk(start, rest, follow_last, 0)
    }

    fn walk(
        &self,
        start: usize,
        path: &[u8],
        follow_last: bool,
        depth: u32,
    ) -> Result<usize, Errno> {
        if depth > SYMLOOP_MAX {
            return Err(Errno(ELOOP as u16));
        }
        let components: Vec<&[u8]> = path
            .split(|&b| b == b'/')
            .filter(|component| !component.is_empty())
            .collect();
        let mut current = start;
        for (index, component) in components.iter().enumerate() {
            let is_last = index.wrapping_add(1) == components.len();
            let Some(node) = self.nodes.get(current) else {
                return Err(Errno(ENOENT as u16));
            };
            if node.kind != NodeKind::Dir {
                return Err(Errno(ENOTDIR as u16));
            }
            if node.mode & S_IXUSR == 0 {
                return Err(Errno(EACCES as u16));
            }
            let Some(next) = self.child_id(current, component) else {
                return Err(Errno(ENOENT as u16));
            };
            let next_node = match self.nodes.get(next) {
                Some(node) => node,
                None => return Err(Errno(ENOENT as u16)),
            };
            if next_node.kind == NodeKind::Symlink && (!is_last || follow_last) {
                let target = next_node.target.clone();
                let base = if target.first() == Some(&b'/') {
                    self.root
                } else {
                    next_node.parent.unwrap_or(self.root)
                };
                let target_path = target.strip_prefix(b"/").unwrap_or(&target);
                current = self.walk(base, target_path, true, depth.wrapping_add(1))?;
            } else {
                current = next;
            }
        }
        Ok(current)
    }

    fn stat_of(&self, id: usize) -> Stat {
        match self.nodes.get(id) {
            Some(node) => Stat {
                st_dev: node.dev,
                st_ino: node.ino,
                st_mode: node.mode,
                st_nlink: node.nlink,
                st_uid: 0,
                st_gid: 0,
                st_rdev: 0,
                __pad1: 0,
                st_size: if node.kind == NodeKind::File {
                    node.size
                } else {
                    0
                },
                st_blksize: 4096,
                __pad2: 0,
                st_blocks: node.size / 512,
                st_atim: Timespec {
                    tv_sec: node.mtime,
                    tv_nsec: 0,
                },
                st_mtim: Timespec {
                    tv_sec: node.mtime,
                    tv_nsec: 0,
                },
                st_ctim: Timespec {
                    tv_sec: node.mtime,
                    tv_nsec: 0,
                },
                __unused: [0; 2],
            },
            None => Stat::ZERO,
        }
    }

    fn tick(
        &mut self,
        method: &'static str,
        dirfd: i32,
        name: &[u8],
        flags: u32,
    ) -> Result<(), Errno> {
        self.ops = self.ops.wrapping_add(1);
        self.log.push(Call {
            method,
            dirfd,
            name: name.to_vec(),
            flags,
        });
        if let Some((after, errno)) = self.fail
            && self.ops == after
        {
            self.fail = None;
            return Err(errno);
        }
        // A hook scheduled for the previous call runs now, after that call completed and before
        // this one starts: the observable "between two calls" point.
        if self
            .hook
            .as_ref()
            .is_some_and(|(after, _)| after.wrapping_add(1) == self.ops)
            && let Some((_, mut hook)) = self.hook.take()
        {
            hook(self);
        }
        Ok(())
    }

    fn alloc_fd(&mut self, node: usize, is_path: bool) -> Result<Fd, Errno> {
        let open = self.fds.iter().filter(|slot| slot.is_some()).count();
        if let Some(limit) = self.emfile
            && open >= limit
        {
            return Err(Errno(EMFILE as u16));
        }
        let fd = OpenFd {
            node,
            offset: 0,
            is_path,
        };
        let index = match self.fds.iter().position(|slot| slot.is_none()) {
            Some(index) => {
                if let Some(slot) = self.fds.get_mut(index) {
                    *slot = Some(fd);
                }
                index
            }
            None => {
                self.fds.push(Some(fd));
                self.fds.len().saturating_sub(1)
            }
        };
        Ok(Fd(index as i32))
    }

    fn record_len(&self, dir: usize, index: usize) -> Option<(Vec<u8>, u8, u64)> {
        let node = self.nodes.get(dir)?;
        match index {
            0 => Some((b".".to_vec(), DT_DIR as u8, node.ino)),
            1 => {
                let parent = node.parent.unwrap_or(dir);
                let ino = self.nodes.get(parent).map_or(node.ino, |node| node.ino);
                Some((b"..".to_vec(), DT_DIR as u8, ino))
            }
            n => {
                let (name, child) = node.children.get(n.wrapping_sub(2))?;
                let (kind, ino) = self
                    .nodes
                    .get(*child)
                    .map_or((NodeKind::File, 0), |node| (node.kind, node.ino));
                Some((name.clone(), dirent_type(kind), ino))
            }
        }
    }
}

impl Default for FakeFs {
    fn default() -> Self {
        Self::new()
    }
}

fn dirent_reclen(name_len: usize) -> usize {
    let raw = 19usize.wrapping_add(name_len).wrapping_add(1);
    raw.wrapping_add(7) & !7
}

fn put_u64(buf: &mut [u8], pos: usize, value: u64) {
    let end = pos.wrapping_add(8);
    if let Some(slot) = buf.get_mut(pos..end) {
        slot.copy_from_slice(&value.to_le_bytes());
    }
}

fn put_i64(buf: &mut [u8], pos: usize, value: i64) {
    put_u64(buf, pos, value as u64);
}

fn put_u16(buf: &mut [u8], pos: usize, value: u16) {
    let end = pos.wrapping_add(2);
    if let Some(slot) = buf.get_mut(pos..end) {
        slot.copy_from_slice(&value.to_le_bytes());
    }
}

fn put_u8(buf: &mut [u8], pos: usize, value: u8) {
    if let Some(slot) = buf.get_mut(pos) {
        *slot = value;
    }
}

impl DirSys for FakeFs {
    fn openat(&mut self, dirfd: i32, name: &[u8], flags: u32) -> Result<Fd, Errno> {
        self.tick("openat", dirfd, name, flags)?;
        if name.is_empty() {
            return Err(Errno(ENOENT as u16));
        }
        let nofollow = flags & (O_NOFOLLOW as u32) != 0;
        let is_path = flags & (O_PATH as u32) != 0;
        let base = if name.first() == Some(&b'/') {
            self.root
        } else if dirfd == AT_FDCWD {
            self.cwd
        } else {
            match self.fds.get(dirfd as usize).and_then(|slot| slot.as_ref()) {
                Some(fd) => fd.node,
                None => return Err(Errno(EBADF as u16)),
            }
        };
        let id = self.walk(base, name, !nofollow, 0)?;
        let kind = match self.nodes.get(id) {
            Some(node) => node.kind,
            None => return Err(Errno(ENOENT as u16)),
        };
        // Linux `open(O_NOFOLLOW)` on a symlink fails with ELOOP; `O_PATH|O_NOFOLLOW` opens the
        // link itself.
        if nofollow && kind == NodeKind::Symlink && !is_path {
            return Err(Errno(ELOOP as u16));
        }
        if flags & (O_DIRECTORY as u32) != 0 && kind != NodeKind::Dir {
            return Err(Errno(ENOTDIR as u16));
        }
        if !is_path && kind == NodeKind::Dir {
            let mode = self.nodes.get(id).map_or(0, |node| node.mode);
            if mode & S_IRUSR == 0 {
                return Err(Errno(EACCES as u16));
            }
        }
        self.alloc_fd(id, is_path)
    }

    fn fstatat(&mut self, dirfd: i32, name: &[u8], nofollow: bool) -> Result<Stat, Errno> {
        self.tick("fstatat", dirfd, name, 0)?;
        if name.is_empty() {
            return Err(Errno(ENOENT as u16));
        }
        let base = if name.first() == Some(&b'/') {
            self.root
        } else if dirfd == AT_FDCWD {
            self.cwd
        } else {
            match self.fds.get(dirfd as usize).and_then(|slot| slot.as_ref()) {
                Some(fd) => fd.node,
                None => return Err(Errno(EBADF as u16)),
            }
        };
        let id = self.walk(base, name, !nofollow, 0)?;
        Ok(self.stat_of(id))
    }

    fn fstat(&mut self, fd: Fd) -> Result<Stat, Errno> {
        self.tick("fstat", fd.0, &[], 0)?;
        match self.fds.get(fd.0 as usize).and_then(|slot| slot.as_ref()) {
            Some(open) => Ok(self.stat_of(open.node)),
            None => Err(Errno(EBADF as u16)),
        }
    }

    fn getdents(&mut self, fd: Fd, buf: &mut [u8]) -> Result<usize, Errno> {
        self.tick("getdents", fd.0, &[], 0)?;
        let Some(open) = self
            .fds
            .get(fd.0 as usize)
            .and_then(|slot| slot.as_ref())
            .cloned()
        else {
            return Err(Errno(EBADF as u16));
        };
        let Some(node) = self.nodes.get(open.node) else {
            return Err(Errno(ENOENT as u16));
        };
        if node.kind != NodeKind::Dir {
            return Err(Errno(ENOTDIR as u16));
        }
        let count = node.children.len().wrapping_add(2);
        let cap = match self.dirent_limit {
            Some(limit) => buf.len().min(limit),
            None => buf.len(),
        };
        let mut index = match usize::try_from(open.offset) {
            Ok(offset) if offset <= count => offset,
            _ => count,
        };
        let mut written = 0usize;
        while index < count {
            let Some((name, typ, ino)) = self.record_len(open.node, index) else {
                break;
            };
            let reclen = dirent_reclen(name.len());
            let end = written.wrapping_add(reclen);
            if end > cap {
                break;
            }
            let next_cookie = index.wrapping_add(1) as i64;
            put_u64(buf, written, ino);
            put_i64(buf, written.wrapping_add(8), next_cookie);
            put_u16(
                buf,
                written.wrapping_add(16),
                u16::try_from(reclen).unwrap_or(u16::MAX),
            );
            put_u8(buf, written.wrapping_add(18), typ);
            if let Some(slot) = buf.get_mut(written.wrapping_add(19)..end) {
                for byte in slot.iter_mut() {
                    *byte = 0;
                }
            }
            if let Some(slot) = buf.get_mut(
                written.wrapping_add(19)..written.wrapping_add(19).wrapping_add(name.len()),
            ) {
                slot.copy_from_slice(&name);
            }
            written = end;
            index = index.wrapping_add(1);
            if let Some(open) = self
                .fds
                .get_mut(fd.0 as usize)
                .and_then(|slot| slot.as_mut())
            {
                open.offset = next_cookie;
            }
        }
        if written == 0 && index < count {
            return Err(Errno(EINVAL as u16));
        }
        Ok(written)
    }

    fn lseek(&mut self, fd: Fd, off: i64, whence: Whence) -> Result<i64, Errno> {
        self.tick("lseek", fd.0, &[], 0)?;
        let Some(open) = self.fds.get(fd.0 as usize).and_then(|slot| slot.as_ref()) else {
            return Err(Errno(EBADF as u16));
        };
        let current = open.offset;
        let node = open.node;
        let is_dir = self
            .nodes
            .get(node)
            .is_some_and(|node| node.kind == NodeKind::Dir);
        let target = match whence {
            Whence::Set => off,
            Whence::Cur => current.saturating_add(off),
            Whence::End => {
                let count = self
                    .nodes
                    .get(node)
                    .map_or(0, |node| node.children.len().wrapping_add(2));
                (count as i64).saturating_add(off)
            }
        };
        if is_dir {
            if target < 0 {
                return Err(Errno(EINVAL as u16));
            }
            let count = self
                .nodes
                .get(node)
                .map_or(0, |node| node.children.len().wrapping_add(2));
            if usize::try_from(target)
                .ok()
                .is_none_or(|value| value > count)
            {
                return Err(Errno(EINVAL as u16));
            }
        }
        if let Some(open) = self
            .fds
            .get_mut(fd.0 as usize)
            .and_then(|slot| slot.as_mut())
        {
            open.offset = target;
        }
        Ok(target)
    }

    fn close(&mut self, fd: Fd) -> Result<(), Errno> {
        self.tick("close", fd.0, &[], 0)?;
        match self.fds.get_mut(fd.0 as usize) {
            Some(slot) => {
                *slot = None;
                Ok(())
            }
            None => Err(Errno(EBADF as u16)),
        }
    }

    fn fcntl_getfl(&mut self, fd: Fd) -> Result<i32, Errno> {
        self.tick("fcntl", fd.0, &[], 0)?;
        match self.fds.get(fd.0 as usize).and_then(|slot| slot.as_ref()) {
            Some(open) => Ok(if open.is_path { O_PATH } else { 0 }),
            None => Err(Errno(EBADF as u16)),
        }
    }
}

#[cfg(test)]
#[path = "fake_ut.rs"]
mod tests;
