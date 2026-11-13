// SPDX-License-Identifier: BSD-3-Clause
//
// Copyright (c) 1990, 1993, 1994
//	The Regents of the University of California.  All rights reserved.
//
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// RavenBSD: transpiled from freebsd:lib/libc/gen/fts.c @ 133344405f532bd54ff3629a77d4c4e062f01fdb
// (file revision tag: $OpenBSD: fts.c,v 1.22 1999/10/03 19:22:22 millert Exp $)
//
// Redistribution and use in source and binary forms, with or without
// modification, are permitted provided that the following conditions
// are met:
// 1. Redistributions of source code must retain the above copyright
//    notice, this list of conditions and the following disclaimer.
// 2. Redistributions in binary form must reproduce the above copyright
//    notice, this list of conditions and the following disclaimer in the
//    documentation and/or other materials provided with the distribution.
// 3. Neither the name of the University nor the names of its contributors
//    may be used to endorse or promote products derived from this software
//    without specific prior written permission.
//
// THIS SOFTWARE IS PROVIDED BY THE REGENTS AND CONTRIBUTORS ``AS IS'' AND
// ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT LIMITED TO, THE
// IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE
// ARE DISCLAIMED.  IN NO EVENT SHALL THE REGENTS OR CONTRIBUTORS BE LIABLE
// FOR ANY DIRECT, INDIRECT, INCIDENTAL, SPECIAL, EXEMPLARY, OR CONSEQUENTIAL
// DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS
// OR SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION)
// HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY, WHETHER IN CONTRACT, STRICT
// LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY
// OUT OF THE USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF
// SUCH DAMAGE.

//! The `fts` walk: `fts_open`, `fts_read`, `fts_set`, `fts_children`, `fts_close`.
//!
//! The donor's node list is an arena here: [`FtsEnt::id`] is the arena index, `read` returns a
//! reference valid until the next `read`/`close`, and `set` takes the id. Every adaptation from
//! the C is marked `// RavenBSD:`; the hardening is H-FTS-1..5 of doc 07 3.5.
//!
//! Stated differences from the donor, all deliberate:
//! * `FTS_NOCHDIR` is forced (H-FTS-1), so there is no `chdir`/`fchdir` and `fts_accpath` is
//!   always `fts_path` (`accpath_is_path`).
//! * Every physical-walk `openat` carries `O_NOFOLLOW` (H-FTS-2), and the opened directory is
//!   `fstat`ed and compared with the recorded `st_dev`/`st_ino` (H-FTS-3).
//! * The path buffer is capped at `PATH_MAX` and an over-long node is `FTS_ERR`/`ENAMETOOLONG`
//!   (H-FTS-5) instead of growing without bound.
//! * An `EMFILE` while opening a directory releases the descriptors of already-open nodes and
//!   retries the full path (H-FTS-4), so a deep tree is still walked with a small fd budget.
//! * `fts_ufslinks`, the `statfs` cache, whiteouts and the block-comparator API are deleted.
//! * Visited nodes are kept in the arena instead of being freed; the C frees them as it walks.

use alloc::boxed::Box;
use alloc::vec::Vec;

use raven_libc_abi::errno::Errno;
use raven_libc_abi::generated::errno::{EINVAL, EMFILE, ENAMETOOLONG, ENFILE, ENOENT};
use raven_libc_abi::generated::fcntl::{O_CLOEXEC, O_DIRECTORY, O_NOFOLLOW, O_RDONLY};
use raven_libc_abi::generated::stat::{
    DT_BLK, DT_CHR, DT_DIR, DT_FIFO, DT_LNK, DT_REG, DT_SOCK, DT_UNKNOWN, S_IFDIR, S_IFLNK, S_IFMT,
    S_IFREG,
};
use raven_libc_abi::generated::structs::Stat;

use crate::consts::*;
use crate::dirent::DirStream;
use crate::{AT_FDCWD, DirSys, Fd, PATH_MAX};

/// An arena index of an [`FtsEnt`].
pub type NodeId = usize;

/// The `fts_open` comparator: `fts_compar` in C, a Rust closure here.
pub type Compar = Box<dyn FnMut(&FtsEnt, &FtsEnt) -> i32>;

/// One node of the walk (the donor's `FTSENT` plus the arena id and the sibling link).
#[derive(Clone)]
pub struct FtsEnt {
    /// The arena index of this node; pass it to [`Fts::set`].
    pub id: NodeId,
    /// `fts_info`: one of `FTS_D`..`FTS_W`.
    pub info: i32,
    /// `fts_flags`: `FTS_DONTCHDIR`/`FTS_SYMFOLLOW`.
    pub flags: u32,
    /// `fts_name`: the last path component (or the root argument before the first read).
    pub name: Vec<u8>,
    /// `fts_path`: the full path.
    pub path: Vec<u8>,
    /// H-FTS-1: `fts_accpath` is always `fts_path`.
    pub accpath_is_path: bool,
    /// `fts_level`: -1 for the root parent, 0 for a root, deeper below.
    pub level: i64,
    /// `fts_errno`: the error recorded for this node.
    pub errno: i32,
    /// `fts_ino`.
    pub ino: u64,
    /// `fts_dev`.
    pub dev: u64,
    /// `fts_nlink`.
    pub nlink: u64,
    /// `fts_number`: client storage.
    pub number: i64,
    /// `fts_pointer`: client storage.
    pub pointer: u64,
    /// `fts_statp`: `None` under `FTS_NOSTAT`.
    pub statp: Option<Stat>,
    /// `fts_parent`.
    pub parent: Option<NodeId>,
    /// `fts_cycle`: the ancestor that caused `FTS_DC`.
    pub cycle: Option<NodeId>,
    /// `fts_dirfd`: -1 when no descriptor is held.
    pub dirfd: i32,
    /// `fts_instr`: the pending `fts_set` instruction.
    pub instr: i32,
}

impl FtsEnt {
    fn new(id: NodeId, name: Vec<u8>, path: Vec<u8>, level: i64) -> Self {
        Self {
            id,
            info: 0,
            flags: 0,
            name,
            path,
            accpath_is_path: true,
            level,
            errno: 0,
            ino: 0,
            dev: 0,
            nlink: 0,
            number: 0,
            pointer: 0,
            statp: None,
            parent: None,
            cycle: None,
            dirfd: NO_FD,
            instr: FTS_NOINSTR,
        }
    }
}

/// A node plus its sibling link (the donor's `fts_link`).
struct Node {
    ent: FtsEnt,
    link: Option<NodeId>,
}

const BCHILD: u32 = 1;
const BNAMES: u32 = 2;
const BREAD: u32 = 3;

/// The walk stream (the donor's `FTS`).
pub struct Fts {
    nodes: Vec<Node>,
    root: Option<NodeId>,
    cur: Option<NodeId>,
    child: Option<NodeId>,
    options: u32,
    compar: Option<Compar>,
    fts_dev: u64,
    stop: bool,
    started: bool,
    root_fd: Option<Fd>,
    /// An invalid [`NodeId`] addresses this node instead of the C's wild pointer.
    fallback: FtsEnt,
}

/// `O_RDONLY|O_DIRECTORY|O_CLOEXEC`.
fn dir_open_flags() -> u32 {
    (O_RDONLY | O_DIRECTORY | O_CLOEXEC) as u32
}

/// Directory flags for a child of a physical walk: `O_NOFOLLOW` (H-FTS-2).
fn dir_flags(options: u32) -> u32 {
    let mut flags = dir_open_flags();
    if options & FTS_LOGICAL == 0 {
        flags |= O_NOFOLLOW as u32;
    }
    flags
}

/// Directory flags for a command-line root: `O_NOFOLLOW` only when nothing follows links.
fn root_flags(options: u32) -> u32 {
    let mut flags = dir_open_flags();
    if options & (FTS_LOGICAL | FTS_COMFOLLOW | FTS_COMFOLLOWDIR) == 0 {
        flags |= O_NOFOLLOW as u32;
    }
    flags
}

fn is_dir(mode: u32) -> bool {
    mode & S_IFMT == S_IFDIR
}

fn is_link(mode: u32) -> bool {
    mode & S_IFMT == S_IFLNK
}

fn is_reg(mode: u32) -> bool {
    mode & S_IFMT == S_IFREG
}

fn is_dot(name: &[u8]) -> bool {
    name == b"." || name == b".."
}

fn is_emfile(errno: Errno) -> bool {
    errno.0 == EMFILE as u16 || errno.0 == ENFILE as u16
}

/// `NAPPEND`: the parent path length, minus a trailing slash.
fn nappend(path: &[u8]) -> usize {
    match path.last() {
        Some(&b'/') => path.len().saturating_sub(1),
        _ => path.len(),
    }
}

/// `fts_path` of a child: the parent path with one separator and the name.
fn child_path(parent: &[u8], name: &[u8]) -> Vec<u8> {
    let mut path = Vec::new();
    let base = nappend(parent);
    if let Some(prefix) = parent.get(..base) {
        path.extend_from_slice(prefix);
    }
    path.push(b'/');
    path.extend_from_slice(name);
    path
}

impl Fts {
    fn ent(&self, id: NodeId) -> &FtsEnt {
        match self.nodes.get(id) {
            Some(node) => &node.ent,
            None => &self.fallback,
        }
    }

    fn ent_mut(&mut self, id: NodeId) -> &mut FtsEnt {
        if self.nodes.get(id).is_some() {
            match self.nodes.get_mut(id) {
                Some(node) => &mut node.ent,
                None => &mut self.fallback,
            }
        } else {
            &mut self.fallback
        }
    }

    fn push_node(&mut self, ent: FtsEnt) -> NodeId {
        let id = self.nodes.len();
        self.nodes.push(Node {
            ent: FtsEnt { id, ..ent },
            link: None,
        });
        id
    }

    fn set_link(&mut self, id: NodeId, link: Option<NodeId>) {
        if let Some(node) = self.nodes.get_mut(id) {
            node.link = link;
        }
    }

    fn link_of(&self, id: NodeId) -> Option<NodeId> {
        self.nodes.get(id).and_then(|node| node.link)
    }

    /// The `fts_options` value (H-FTS-1 keeps `FTS_NOCHDIR` visible).
    #[must_use]
    pub fn options(&self) -> u32 {
        self.options
    }

    /// `fts_openat`: validate the options and paths and stat the roots.
    ///
    /// # Errors
    ///
    /// `EINVAL` for an unknown option bit or an empty `argv`, `ENAMETOOLONG` for a root longer
    /// than `PATH_MAX`, or the backend's error for the root descriptor.
    pub fn open<S: DirSys>(
        sys: &mut S,
        dirfd: i32,
        argv: &[&[u8]],
        options: u32,
        compar: Option<Compar>,
    ) -> Result<Fts, Errno> {
        if options & !FTS_OPTIONMASK != 0 {
            return Err(Errno(EINVAL as u16));
        }
        if argv.is_empty() {
            return Err(Errno(EINVAL as u16));
        }
        for arg in argv {
            if arg.len() > PATH_MAX {
                return Err(Errno(ENAMETOOLONG as u16));
            }
        }
        // RavenBSD: H-FTS-1: FTS_NOCHDIR is forced, so no fchdir path exists.
        let options = options | FTS_NOCHDIR;
        let options = if options & FTS_NOSTAT_TYPE != 0 {
            options | FTS_NOSTAT
        } else {
            options
        };
        let root_fd = if dirfd == AT_FDCWD {
            None
        } else {
            Some(sys.openat(dirfd, b".", dir_open_flags())?)
        };
        let mut fts = Fts {
            nodes: Vec::new(),
            root: None,
            cur: None,
            child: None,
            options,
            compar,
            fts_dev: 0,
            stop: false,
            started: false,
            root_fd,
            fallback: FtsEnt::new(0, Vec::new(), Vec::new(), FTS_ROOTPARENTLEVEL),
        };
        let parent_id = fts.push_node(FtsEnt::new(0, Vec::new(), Vec::new(), FTS_ROOTPARENTLEVEL));
        fts.ent_mut(parent_id).dirfd = match root_fd {
            Some(fd) => fd.0,
            None => AT_FDCWD,
        };

        let follow = if options & FTS_COMFOLLOWDIR != 0 {
            -1
        } else if options & FTS_COMFOLLOW != 0 {
            1
        } else {
            0
        };
        let dfd = match root_fd {
            Some(fd) => fd.0,
            None => -1,
        };

        let mut roots: Vec<NodeId> = Vec::new();
        for arg in argv {
            let id = fts.push_node(FtsEnt::new(
                fts.nodes.len(),
                arg.to_vec(),
                arg.to_vec(),
                FTS_ROOTLEVEL,
            ));
            fts.ent_mut(id).parent = Some(parent_id);
            let info = fts.stat_node(sys, id, follow, dfd);
            fts.ent_mut(id).info = info;
            if info == FTS_DOT {
                fts.ent_mut(id).info = FTS_D;
            }
            if fts.ent(id).info == FTS_D {
                let flags = root_flags(options);
                let fd = match root_fd {
                    Some(root) => sys.openat(root.0, arg, flags),
                    None => sys.openat(AT_FDCWD, arg, flags),
                };
                match fd {
                    Ok(fd) => {
                        fts.ent_mut(id).dirfd = fd.0;
                        fts.check_identity(sys, id);
                    }
                    Err(_) => fts.ent_mut(id).dirfd = NO_FD,
                }
            }
            roots.push(id);
        }

        if roots.len() > 1 && fts.compar.is_some() {
            let head = fts.relink(&roots);
            fts.root = fts.sort_list(head, roots.len());
        } else {
            fts.root = fts.relink(&roots);
        }
        Ok(fts)
    }

    fn relink(&mut self, nodes: &[NodeId]) -> Option<NodeId> {
        let mut previous: Option<NodeId> = None;
        for &id in nodes {
            if let Some(previous) = previous {
                self.set_link(previous, Some(id));
            }
            previous = Some(id);
        }
        if let Some(last) = previous {
            self.set_link(last, None);
        }
        nodes.first().copied()
    }

    /// H-FTS-3: the opened descriptor must be the node `fts_stat` recorded.
    fn check_identity<S: DirSys>(&mut self, sys: &mut S, id: NodeId) {
        let fd = self.ent(id).dirfd;
        if fd < 0 {
            return;
        }
        let expected = (self.ent(id).dev, self.ent(id).ino);
        let ok = match sys.fstat(Fd(fd)) {
            Ok(stat) => stat.st_dev == expected.0 && stat.st_ino == expected.1,
            Err(_) => false,
        };
        if !ok {
            let _ = sys.close(Fd(fd));
            let ent = self.ent_mut(id);
            ent.dirfd = NO_FD;
            ent.info = FTS_ERR;
            // RavenBSD: H-FTS-3: the donor's "disinformation" errno (fts_safe_changedir).
            ent.errno = ENOENT;
        }
    }

    /// H-FTS-4: close every node descriptor (the root parent's is the separately owned root fd).
    fn release_dirfds<S: DirSys>(&mut self, sys: &mut S) {
        for node in self.nodes.iter_mut().skip(1) {
            if node.ent.dirfd >= 0 {
                let _ = sys.close(Fd(node.ent.dirfd));
                node.ent.dirfd = NO_FD;
            }
        }
        self.child = None;
    }

    /// `fts_load`: trim a root argument to its last component and record its device.
    fn load(&mut self, id: NodeId) {
        let name = self.ent(id).name.clone();
        if let Some(position) = name.iter().rposition(|&b| b == b'/')
            && (position != 0 || name.get(position.wrapping_add(1)).is_some_and(|&b| b != 0))
        {
            let trimmed = name.get(position.wrapping_add(1)..).unwrap_or(&[]).to_vec();
            self.ent_mut(id).name = trimmed;
        }
        self.fts_dev = self.ent(id).dev;
    }

    /// `fts_stat`: fill the node's identity and return its `FTS_*` info value.
    fn stat_node<S: DirSys>(&mut self, sys: &mut S, id: NodeId, follow: i32, dfd: i32) -> i32 {
        let (path, at_fd) = {
            let ent = self.ent(id);
            if dfd == -1 {
                (ent.path.clone(), AT_FDCWD)
            } else {
                (ent.name.clone(), dfd)
            }
        };
        let mut sb = Stat::ZERO;
        let mut first_ok = false;
        let mut first_err = 0i32;
        let mut need_lstat = false;

        if self.options & FTS_LOGICAL != 0 || follow != 0 {
            match sys.fstatat(at_fd, &path, false) {
                Ok(stat) => {
                    sb = stat;
                    first_ok = true;
                    if follow < 0 && !is_dir(sb.st_mode) {
                        need_lstat = true;
                    }
                }
                Err(error) => {
                    first_err = i32::from(error.0);
                    need_lstat = true;
                }
            }
            if need_lstat {
                match sys.fstatat(at_fd, &path, true) {
                    Ok(stat) => {
                        sb = stat;
                        if !first_ok && is_link(stat.st_mode) {
                            return self.finish_stat(id, FTS_SLNONE, sb);
                        }
                    }
                    Err(_) => {
                        self.ent_mut(id).errno = first_err;
                        return self.finish_stat(id, FTS_NS, Stat::ZERO);
                    }
                }
            }
        } else {
            match sys.fstatat(at_fd, &path, true) {
                Ok(stat) => sb = stat,
                Err(error) => {
                    self.ent_mut(id).errno = i32::from(error.0);
                    return self.finish_stat(id, FTS_NS, Stat::ZERO);
                }
            }
        }

        if is_dir(sb.st_mode) {
            {
                let ent = self.ent_mut(id);
                ent.dev = sb.st_dev;
                ent.ino = sb.st_ino;
                ent.nlink = u64::from(sb.st_nlink);
            }
            if is_dot(&self.ent(id).name) {
                return self.finish_stat(id, FTS_DOT, sb);
            }
            let mut ancestor = self.ent(id).parent;
            while let Some(parent) = ancestor {
                if self.ent(parent).level < FTS_ROOTLEVEL {
                    break;
                }
                if self.ent(parent).ino == sb.st_ino && self.ent(parent).dev == sb.st_dev {
                    self.ent_mut(id).cycle = Some(parent);
                    return self.finish_stat(id, FTS_DC, sb);
                }
                ancestor = self.ent(parent).parent;
            }
            return self.finish_stat(id, FTS_D, sb);
        }
        if is_link(sb.st_mode) {
            return self.finish_stat(id, FTS_SL, sb);
        }
        if is_reg(sb.st_mode) {
            return self.finish_stat(id, FTS_F, sb);
        }
        self.finish_stat(id, FTS_DEFAULT, sb)
    }

    fn finish_stat(&mut self, id: NodeId, info: i32, stat: Stat) -> i32 {
        if self.options & FTS_NOSTAT == 0 {
            self.ent_mut(id).statp = Some(stat);
        }
        self.ent_mut(id).info = info;
        info
    }

    /// `fts_read`: the next pre-order/post-order node, or `None` at the end.
    ///
    /// # Errors
    ///
    /// The [`DirSys`] errors are recorded on the node (`FTS_DNR`/`FTS_ERR`/`FTS_NS`); this
    /// signature keeps the seam uniform.
    pub fn read<S: DirSys>(&mut self, sys: &mut S) -> Result<Option<&FtsEnt>, Errno> {
        if self.stop {
            return Ok(None);
        }
        if !self.started {
            let Some(root) = self.root else {
                self.stop = true;
                return Ok(None);
            };
            self.started = true;
            self.cur = Some(root);
            self.load(root);
            return Ok(Some(self.ent(root)));
        }
        let Some(cur) = self.cur else {
            return Ok(None);
        };

        let instr = self.ent(cur).instr;
        self.ent_mut(cur).instr = FTS_NOINSTR;

        if instr == FTS_AGAIN {
            let info = self.stat_node(sys, cur, 0, -1);
            self.ent_mut(cur).info = info;
            return Ok(Some(self.ent(cur)));
        }

        let cur_info = self.ent(cur).info;
        if instr == FTS_FOLLOW && (cur_info == FTS_SL || cur_info == FTS_SLNONE) {
            let info = self.stat_node(sys, cur, 1, -1);
            self.ent_mut(cur).info = info;
            return Ok(Some(self.ent(cur)));
        }

        if cur_info == FTS_D {
            if instr == FTS_SKIP
                || (self.options & FTS_XDEV != 0 && self.ent(cur).dev != self.fts_dev)
            {
                self.free_child(sys);
                self.ent_mut(cur).info = FTS_DP;
                return Ok(Some(self.ent(cur)));
            }
            if self.child.is_some() && self.options & FTS_NAMEONLY != 0 {
                self.options &= !FTS_NAMEONLY;
                self.free_child(sys);
            }
            if self.child.is_none() {
                let child = self.build(sys, BREAD);
                if child.is_none() {
                    if self.stop {
                        return Ok(None);
                    }
                    return Ok(Some(self.ent(cur)));
                }
            }
            let Some(child) = self.child.take() else {
                return Ok(Some(self.ent(cur)));
            };
            self.cur = Some(child);
            return Ok(Some(self.ent(child)));
        }

        let mut tmp = cur;
        loop {
            let info = self.ent(tmp).info;
            let dirfd = self.ent(tmp).dirfd;
            if dirfd >= 0 && info == FTS_DP {
                let _ = sys.close(Fd(dirfd));
                self.ent_mut(tmp).dirfd = NO_FD;
            }
            let Some(next) = self.link_of(tmp) else {
                break;
            };
            if self.ent(next).level == FTS_ROOTLEVEL {
                self.cur = Some(next);
                self.load(next);
                return Ok(Some(self.ent(next)));
            }
            if self.ent(next).instr == FTS_SKIP {
                tmp = next;
                continue;
            }
            if self.ent(next).instr == FTS_FOLLOW {
                let info = self.stat_node(sys, next, 1, -1);
                self.ent_mut(next).info = info;
                self.ent_mut(next).instr = FTS_NOINSTR;
            }
            self.cur = Some(next);
            return Ok(Some(self.ent(next)));
        }

        let Some(parent) = self.ent(tmp).parent else {
            self.cur = None;
            return Ok(None);
        };
        if self.ent(parent).level == FTS_ROOTPARENTLEVEL {
            let dirfd = self.ent(parent).dirfd;
            if dirfd >= 0 {
                let _ = sys.close(Fd(dirfd));
            }
            self.ent_mut(parent).dirfd = NO_FD;
            self.root_fd = None;
            self.cur = None;
            return Ok(None);
        }
        let info = if self.ent(parent).errno != 0 {
            FTS_ERR
        } else {
            FTS_DP
        };
        self.ent_mut(parent).info = info;
        self.cur = Some(parent);
        Ok(Some(self.ent(parent)))
    }

    /// `fts_build`: read the current directory into a sibling list.
    fn build<S: DirSys>(&mut self, sys: &mut S, kind: u32) -> Option<NodeId> {
        let cur = self.cur?;
        let cur_path = self.ent(cur).path.clone();
        let cur_dirfd = self.ent(cur).dirfd;
        let mut dir = match self.open_stream(sys, cur_dirfd, &cur_path) {
            Ok(dir) => dir,
            Err(error) => {
                if is_emfile(error) {
                    self.release_dirfds(sys);
                    match self.open_stream(sys, cur_dirfd, &cur_path) {
                        Ok(dir) => dir,
                        Err(error) => return self.dir_failed(cur, kind, error),
                    }
                } else {
                    return self.dir_failed(cur, kind, error);
                }
            }
        };
        let stream_fd = dir.dirfd().0;

        let level = self.ent(cur).level.wrapping_add(1);
        let mut nlinks = if kind == BNAMES {
            0
        } else {
            self.link_budget(cur)
        };
        let mut head: Option<NodeId> = None;
        let mut tail: Option<NodeId> = None;
        let mut nitems = 0usize;
        let mut readdir_err = 0i32;
        loop {
            let entry = match dir.readdir(sys) {
                Ok(Some(entry)) => entry,
                Ok(None) => break,
                Err(error) => {
                    readdir_err = i32::from(error.0);
                    break;
                }
            };
            if self.options & FTS_SEEDOT == 0 && is_dot(&entry.name) {
                continue;
            }
            let path = child_path(&cur_path, &entry.name);
            let too_long = path.len() > PATH_MAX;
            let stored_path = if too_long {
                path.get(..PATH_MAX).unwrap_or(&[]).to_vec()
            } else {
                path
            };
            let id = self.push_node(FtsEnt::new(
                self.nodes.len(),
                entry.name.clone(),
                stored_path,
                level,
            ));
            self.ent_mut(id).parent = Some(cur);

            if entry.typ == DT_DIR as u8 {
                match sys.openat(stream_fd, &entry.name, dir_flags(self.options)) {
                    Ok(fd) => self.ent_mut(id).dirfd = fd.0,
                    Err(_) => self.ent_mut(id).dirfd = NO_FD,
                }
            }

            if too_long {
                // RavenBSD: H-FTS-5: cap instead of the donor's unbounded fts_palloc.
                self.ent_mut(id).info = FTS_ERR;
                self.ent_mut(id).errno = ENAMETOOLONG;
            } else {
                let nostat = self.options & FTS_NOSTAT != 0
                    && self.options & FTS_PHYSICAL != 0
                    && nlinks >= 0;
                if nlinks == 0
                    || (nostat && entry.typ != DT_DIR as u8 && entry.typ != DT_UNKNOWN as u8)
                {
                    self.ent_mut(id).info = FTS_NSOK;
                } else {
                    let info = self.stat_node(sys, id, 0, stream_fd);
                    self.ent_mut(id).info = info;
                    if nlinks > 0 && matches!(info, FTS_D | FTS_DC | FTS_DOT) {
                        nlinks = nlinks.saturating_sub(1);
                    }
                }
                if self.ent(id).info == FTS_NSOK && self.options & FTS_NOSTAT_TYPE != 0 {
                    let info = match entry.typ {
                        t if t == DT_FIFO as u8
                            || t == DT_CHR as u8
                            || t == DT_BLK as u8
                            || t == DT_SOCK as u8 =>
                        {
                            FTS_DEFAULT
                        }
                        t if t == DT_REG as u8 => FTS_F,
                        t if t == DT_LNK as u8 => FTS_SL,
                        _ => FTS_NSOK,
                    };
                    self.ent_mut(id).info = info;
                }
            }

            if self.ent(id).info == FTS_D {
                self.check_identity(sys, id);
            }
            if self.ent(id).info != FTS_D && self.ent(id).dirfd >= 0 {
                // A directory-typed entry whose stat disagreed: do not hold its descriptor.
                let fd = self.ent(id).dirfd;
                let _ = sys.close(Fd(fd));
                self.ent_mut(id).dirfd = NO_FD;
            }

            self.set_link(id, None);
            match tail {
                Some(tail_id) => self.set_link(tail_id, Some(id)),
                None => head = Some(id),
            }
            tail = Some(id);
            nitems = nitems.wrapping_add(1);
        }

        if readdir_err != 0 {
            self.ent_mut(cur).errno = readdir_err;
            self.ent_mut(cur).info = if nitems > 0 { FTS_ERR } else { FTS_DNR };
        }
        let _ = dir.closedir(sys);

        if nitems == 0 {
            if kind == BREAD && self.ent(cur).info != FTS_DNR && self.ent(cur).info != FTS_ERR {
                self.ent_mut(cur).info = FTS_DP;
            }
            self.child = None;
            return None;
        }
        if nitems > 1 && self.compar.is_some() {
            head = self.sort_list(head, nitems);
        }
        self.child = head;
        head
    }

    fn dir_failed(&mut self, cur: NodeId, kind: u32, error: Errno) -> Option<NodeId> {
        if kind == BREAD {
            let ent = self.ent_mut(cur);
            ent.info = FTS_DNR;
            ent.errno = i32::from(error.0);
        }
        self.child = None;
        None
    }

    /// The donor's `nlinks` budget for the current directory (the fstype check is deleted).
    fn link_budget(&self, cur: NodeId) -> i64 {
        if self.options & FTS_NOSTAT == 0 || self.options & FTS_PHYSICAL == 0 {
            return -1;
        }
        let nlink = self.ent(cur).nlink;
        if nlink >= 2 && nlink < u64::from(i32::MAX as u32) {
            let nlink = nlink as i64;
            let subtract = if self.options & FTS_SEEDOT != 0 { 0 } else { 2 };
            nlink.saturating_sub(subtract)
        } else {
            -1
        }
    }

    /// Open the current directory: `openat(cur.dirfd, ".")` then the full-path fallback.
    fn open_stream<S: DirSys>(
        &self,
        sys: &mut S,
        dirfd: i32,
        path: &[u8],
    ) -> Result<DirStream, Errno> {
        if dirfd >= 0
            && let Ok(fd) = sys.openat(dirfd, b".", dir_flags(self.options))
        {
            return match DirStream::fdopendir(sys, fd) {
                Ok(dir) => Ok(dir),
                Err(error) => {
                    let _ = sys.close(fd);
                    Err(error)
                }
            };
        }
        DirStream::opendir(sys, path)
    }

    fn free_child<S: DirSys>(&mut self, sys: &mut S) {
        let mut current = self.child.take();
        while let Some(id) = current {
            let dirfd = self.ent(id).dirfd;
            if dirfd >= 0 {
                let _ = sys.close(Fd(dirfd));
                self.ent_mut(id).dirfd = NO_FD;
            }
            current = self.link_of(id);
        }
    }

    /// `fts_sort`: merge sort of a node list, never panicking on a bad comparator.
    fn sort_list(&mut self, head: Option<NodeId>, nitems: usize) -> Option<NodeId> {
        if nitems < 2 {
            return head;
        }
        let mut ids: Vec<NodeId> = Vec::new();
        let mut current = head;
        while let Some(id) = current {
            ids.push(id);
            current = self.link_of(id);
        }
        let Some(mut compare) = self.compar.take() else {
            return head;
        };
        {
            let nodes = &self.nodes;
            let mut closure = |a: &NodeId, b: &NodeId| match (nodes.get(*a), nodes.get(*b)) {
                (Some(left), Some(right)) => compare(&left.ent, &right.ent),
                _ => 0,
            };
            crate::sort::merge_sort(&mut ids, &mut closure);
        }
        self.compar = Some(compare);
        let first = ids.first().copied();
        self.relink(&ids);
        first
    }

    /// `fts_children`: the current directory's children (or the root list before the first read).
    ///
    /// # Errors
    ///
    /// `EINVAL` for an instruction other than `0`/`FTS_NAMEONLY`.
    pub fn children<S: DirSys>(&mut self, sys: &mut S, instr: i32) -> Result<Vec<&FtsEnt>, Errno> {
        if instr != 0 && instr != FTS_NAMEONLY as i32 {
            return Err(Errno(EINVAL as u16));
        }
        if !self.started {
            return Ok(self.list_refs(self.root));
        }
        let Some(cur) = self.cur else {
            return Ok(Vec::new());
        };
        if self.stop || self.ent(cur).info != FTS_D {
            return Ok(Vec::new());
        }
        self.free_child(sys);
        let kind = if instr == FTS_NAMEONLY as i32 {
            self.options |= FTS_NAMEONLY;
            BNAMES
        } else {
            BCHILD
        };
        let _head = self.build(sys, kind);
        Ok(self.list_refs(self.child))
    }

    fn list_refs(&self, head: Option<NodeId>) -> Vec<&FtsEnt> {
        let mut out = Vec::new();
        let mut current = head;
        while let Some(id) = current {
            out.push(self.ent(id));
            current = self.link_of(id);
        }
        out
    }

    /// `fts_set`: store an instruction for the node.
    ///
    /// # Errors
    ///
    /// `EINVAL` for an instruction other than `0`, `FTS_AGAIN`, `FTS_FOLLOW`, `FTS_NOINSTR` or
    /// `FTS_SKIP`.
    pub fn set(&mut self, ent: NodeId, instr: i32) -> Result<(), Errno> {
        if instr != 0
            && instr != FTS_AGAIN
            && instr != FTS_FOLLOW
            && instr != FTS_NOINSTR
            && instr != FTS_SKIP
        {
            return Err(Errno(EINVAL as u16));
        }
        if self.nodes.get(ent).is_none() {
            return Err(Errno(EINVAL as u16));
        }
        self.ent_mut(ent).instr = instr;
        Ok(())
    }

    /// `fts_close`: close every descriptor the walk owns.
    ///
    /// # Errors
    ///
    /// Always `Ok` in this port; the donor reports a failed `fchdir`, which does not exist here.
    pub fn close<S: DirSys>(mut self, sys: &mut S) -> Result<(), Errno> {
        if let Some(fd) = self.root_fd.take() {
            let _ = sys.close(fd);
        }
        for node in self.nodes.iter_mut().skip(1) {
            if node.ent.dirfd >= 0 {
                let _ = sys.close(Fd(node.ent.dirfd));
                node.ent.dirfd = NO_FD;
            }
        }
        self.ent_mut(0).dirfd = NO_FD;
        Ok(())
    }
}

/// The `fts_open` spelling: `fts_openat(AT_FDCWD, ...)`.
///
/// # Errors
///
/// The same errors as [`Fts::open`].
pub fn fts_open<S: DirSys>(
    sys: &mut S,
    argv: &[&[u8]],
    options: u32,
    compar: Option<Compar>,
) -> Result<Fts, Errno> {
    Fts::open(sys, AT_FDCWD, argv, options, compar)
}

#[cfg(test)]
#[path = "fts_ut.rs"]
mod tests;
