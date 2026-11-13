// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! A path-based fake filesystem and environment for the glob integration tests.
//!
//! This is the "~100-line path-based fake" of C46 step 7: directories, files, symlinks,
//! permissions, injectable errors and per-method call counters. No real filesystem is touched.
#![allow(dead_code)]

use std::cell::Cell;
use std::collections::BTreeMap;

use raven_libc_abi::errno::Errno;
use raven_libc_abi::generated::errno::{EACCES, EINVAL, ELOOP, ENOENT, ENOSYS, ENOTDIR};
use raven_libc_abi::generated::stat::{
    S_IFBLK, S_IFCHR, S_IFDIR, S_IFIFO, S_IFLNK, S_IFMT, S_IFREG, S_IFSOCK, S_IRUSR, S_IXUSR,
};
use raven_libc_abi::generated::structs::{Stat, Timespec};
use raven_libc_glob::{DirHandle, DirSys, GlobEnv, GlobState, PatternLocale, Utf8Locale};

pub const SYMLOOP_MAX: u32 = 40;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Kind {
    Dir,
    File,
    Symlink,
    Fifo,
    Char,
    Block,
    Socket,
}

pub struct Node {
    kind: Kind,
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

pub struct Fake {
    nodes: Vec<Node>,
    root: usize,
    cwd: usize,
    next_ino: u64,
    mtime: i64,
    open_dirs: Vec<OpenDir>,
    pub lstat_calls: usize,
    pub stat_calls: usize,
    pub readdir_calls: usize,
    pub opendir_calls: usize,
    fail_lstat: Option<(usize, i32)>,
    fail_readdir: Option<(usize, i32)>,
    fail_opendir: Option<(usize, i32)>,
}

struct OpenDir {
    node: usize,
    pos: usize,
}

fn type_mode(kind: Kind) -> u32 {
    match kind {
        Kind::Dir => S_IFDIR,
        Kind::File => S_IFREG,
        Kind::Symlink => S_IFLNK,
        Kind::Fifo => S_IFIFO,
        Kind::Char => S_IFCHR,
        Kind::Block => S_IFBLK,
        Kind::Socket => S_IFSOCK,
    }
}

impl Fake {
    pub fn new() -> Self {
        let root = Node {
            kind: Kind::Dir,
            mode: 0o755,
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
            nodes: vec![root],
            root: 0,
            cwd: 0,
            next_ino: 2,
            mtime: 0,
            open_dirs: Vec::new(),
            lstat_calls: 0,
            stat_calls: 0,
            readdir_calls: 0,
            opendir_calls: 0,
            fail_lstat: None,
            fail_readdir: None,
            fail_opendir: None,
        }
    }

    fn alloc(&mut self, kind: Kind, parent: usize, name: &[u8], mode: u32) -> usize {
        let ino = self.next_ino;
        self.next_ino += 1;
        self.mtime += 1;
        let dev = self.nodes[parent].dev;
        let id = self.nodes.len();
        self.nodes.push(Node {
            kind,
            mode: type_mode(kind) | mode,
            dev,
            ino,
            nlink: if kind == Kind::Dir { 2 } else { 1 },
            size: 0,
            mtime: self.mtime,
            parent: Some(parent),
            children: Vec::new(),
            target: Vec::new(),
        });
        self.nodes[parent].children.push((name.to_vec(), id));
        if kind == Kind::Dir {
            self.nodes[parent].nlink += 1;
        }
        id
    }

    fn parent_of(&self, path: &[u8]) -> (usize, Vec<u8>) {
        let path = path.strip_prefix(b"/").unwrap_or(path);
        let split = path.iter().rposition(|&b| b == b'/').map_or(0, |i| i + 1);
        let parent_path = path.get(..split.saturating_sub(1)).unwrap_or(&[]);
        let name = path.get(split..).unwrap_or(&[]).to_vec();
        let parent = self.resolve(parent_path, true).unwrap();
        (parent, name)
    }

    pub fn mkdir(&mut self, path: impl AsRef<[u8]>, mode: u32) {
        let (parent, name) = self.parent_of(path.as_ref());
        self.alloc(Kind::Dir, parent, &name, mode);
    }

    pub fn file(&mut self, path: impl AsRef<[u8]>, size: i64) {
        let (parent, name) = self.parent_of(path.as_ref());
        let id = self.alloc(Kind::File, parent, &name, 0o644);
        self.nodes[id].size = size;
        self.nodes[id].mtime += 1;
    }

    pub fn node(&mut self, path: impl AsRef<[u8]>, kind: Kind) {
        let (parent, name) = self.parent_of(path.as_ref());
        self.alloc(kind, parent, &name, 0o644);
    }

    pub fn symlink(&mut self, path: impl AsRef<[u8]>, target: impl AsRef<[u8]>) {
        let (parent, name) = self.parent_of(path.as_ref());
        let id = self.alloc(Kind::Symlink, parent, &name, 0o777);
        self.nodes[id].target = target.as_ref().to_vec();
    }

    pub fn chmod(&mut self, path: impl AsRef<[u8]>, mode: u32) {
        let id = self.resolve(path.as_ref(), true).unwrap();
        let ty = self.nodes[id].mode & S_IFMT;
        self.nodes[id].mode = ty | mode;
    }

    pub fn remove(&mut self, path: impl AsRef<[u8]>) {
        let (parent, name) = self.parent_of(path.as_ref());
        let index = self.nodes[parent]
            .children
            .iter()
            .position(|(n, _)| n == &name)
            .unwrap();
        self.nodes[parent].children.remove(index);
    }

    pub fn mount(&mut self, path: impl AsRef<[u8]>, dev: u64) {
        let id = self.resolve(path.as_ref(), true).unwrap();
        self.set_dev(id, dev);
    }

    fn set_dev(&mut self, id: usize, dev: u64) {
        self.nodes[id].dev = dev;
        let children: Vec<usize> = self.nodes[id].children.iter().map(|&(_, c)| c).collect();
        for child in children {
            self.set_dev(child, dev);
        }
    }

    pub fn set_cwd(&mut self, path: impl AsRef<[u8]>) {
        self.cwd = self.resolve(path.as_ref(), true).unwrap();
    }

    pub fn entry_count(&self, path: &[u8]) -> usize {
        let id = self.resolve(path, true).unwrap();
        self.nodes[id].children.len()
    }

    pub fn fail_lstat_after(&mut self, after: usize, errno: i32) {
        self.fail_lstat = Some((after, errno));
    }

    pub fn fail_readdir_after(&mut self, after: usize, errno: i32) {
        self.fail_readdir = Some((after, errno));
    }

    pub fn fail_opendir_after(&mut self, after: usize, errno: i32) {
        self.fail_opendir = Some((after, errno));
    }

    fn child(&self, dir: usize, name: &[u8]) -> Option<usize> {
        if name == b"." {
            return Some(dir);
        }
        if name == b".." {
            return Some(self.nodes[dir].parent.unwrap_or(self.root));
        }
        self.nodes[dir]
            .children
            .iter()
            .find(|(n, _)| n == name)
            .map(|&(_, id)| id)
    }

    fn resolve(&self, path: &[u8], follow_last: bool) -> Result<usize, Errno> {
        let (start, path) = if path.first() == Some(&b'/') {
            (self.root, path.strip_prefix(b"/").unwrap_or(path))
        } else {
            (self.cwd, path)
        };
        self.walk(start, path, follow_last, 0)
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
            .filter(|c| !c.is_empty())
            .collect();
        let mut cur = start;
        for (index, component) in components.iter().enumerate() {
            let is_last = index + 1 == components.len();
            if self.nodes[cur].kind != Kind::Dir {
                return Err(Errno(ENOTDIR as u16));
            }
            if self.nodes[cur].mode & S_IXUSR == 0 {
                return Err(Errno(EACCES as u16));
            }
            let Some(next) = self.child(cur, component) else {
                return Err(Errno(ENOENT as u16));
            };
            if self.nodes[next].kind == Kind::Symlink && (!is_last || follow_last) {
                let target = self.nodes[next].target.clone();
                let base = if target.first() == Some(&b'/') {
                    self.root
                } else {
                    self.nodes[next].parent.unwrap_or(self.root)
                };
                let target_path = target.strip_prefix(b"/").unwrap_or(&target);
                cur = self.walk(base, target_path, true, depth + 1)?;
            } else {
                cur = next;
            }
        }
        Ok(cur)
    }

    fn stat_of(&self, id: usize) -> Stat {
        let node = &self.nodes[id];
        Stat {
            st_dev: node.dev,
            st_ino: node.ino,
            st_mode: node.mode,
            st_nlink: node.nlink,
            st_uid: 0,
            st_gid: 0,
            st_rdev: 0,
            __pad1: 0,
            st_size: if node.kind == Kind::File {
                node.size
            } else {
                0
            },
            st_blksize: 4096,
            __pad2: 0,
            st_blocks: (node.size + 511) / 512,
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
        }
    }

    fn check_fail(counter: usize, slot: &mut Option<(usize, i32)>) -> Result<(), Errno> {
        if let Some((after, errno)) = *slot
            && counter == after
        {
            *slot = None;
            return Err(Errno(errno as u16));
        }
        Ok(())
    }
}

impl Default for Fake {
    fn default() -> Self {
        Self::new()
    }
}

impl DirSys for Fake {
    fn opendir(&mut self, path: &[u8]) -> Result<DirHandle, Errno> {
        self.opendir_calls += 1;
        Self::check_fail(self.opendir_calls, &mut self.fail_opendir)?;
        let id = self.resolve(path, true)?;
        if self.nodes[id].kind != Kind::Dir {
            return Err(Errno(ENOTDIR as u16));
        }
        if self.nodes[id].mode & S_IRUSR == 0 {
            return Err(Errno(EACCES as u16));
        }
        let handle = DirHandle(self.open_dirs.len());
        self.open_dirs.push(OpenDir { node: id, pos: 0 });
        Ok(handle)
    }

    fn readdir(&mut self, d: &mut DirHandle) -> Result<Option<Vec<u8>>, Errno> {
        self.readdir_calls += 1;
        Self::check_fail(self.readdir_calls, &mut self.fail_readdir)?;
        let Some(dir) = self.open_dirs.get_mut(d.0) else {
            return Err(Errno(EINVAL as u16));
        };
        let names: Vec<u8> = {
            let node = &self.nodes[dir.node];
            match dir.pos {
                0 => b".".to_vec(),
                1 => b"..".to_vec(),
                n => match node.children.get(n - 2) {
                    Some((name, _)) => name.clone(),
                    None => return Ok(None),
                },
            }
        };
        dir.pos += 1;
        Ok(Some(names))
    }

    fn closedir(&mut self, _d: DirHandle) {}

    fn lstat(&mut self, path: &[u8]) -> Result<Stat, Errno> {
        self.lstat_calls += 1;
        Self::check_fail(self.lstat_calls, &mut self.fail_lstat)?;
        let id = self.resolve(path, false)?;
        Ok(self.stat_of(id))
    }

    fn stat(&mut self, path: &[u8]) -> Result<Stat, Errno> {
        self.stat_calls += 1;
        let id = self.resolve(path, true)?;
        Ok(self.stat_of(id))
    }
}

pub struct FakeEnv {
    pub home: Option<Vec<u8>>,
    pub login: Option<Vec<u8>>,
    pub passwd: BTreeMap<Vec<u8>, Vec<u8>>,
    pub uid_dir: Option<Vec<u8>>,
    pub home_calls: Cell<usize>,
}

impl FakeEnv {
    pub fn new() -> Self {
        Self {
            home: None,
            login: None,
            passwd: BTreeMap::new(),
            uid_dir: None,
            home_calls: Cell::new(0),
        }
    }

    pub fn with_home(home: &[u8]) -> Self {
        let mut env = Self::new();
        env.home = Some(home.to_vec());
        env
    }
}

impl Default for FakeEnv {
    fn default() -> Self {
        Self::new()
    }
}

impl GlobEnv for FakeEnv {
    fn secure_getenv_home(&self) -> Option<Vec<u8>> {
        self.home_calls.set(self.home_calls.get() + 1);
        self.home.clone()
    }

    fn getlogin(&self) -> Option<Vec<u8>> {
        self.login.clone()
    }

    fn getpwnam_dir(&self, name: &[u8]) -> Option<Vec<u8>> {
        self.passwd.get(name).cloned()
    }

    fn getpwuid_self_dir(&self) -> Option<Vec<u8>> {
        self.uid_dir.clone()
    }
}

pub fn run(fs: &mut Fake, env: &FakeEnv, pattern: &[u8], flags: u32, out: &mut GlobState) -> i32 {
    let loc = Utf8Locale;
    raven_libc_glob::glob(fs, env, &loc, pattern, flags, None, out)
}

pub fn run_locale(
    fs: &mut Fake,
    env: &FakeEnv,
    loc: &dyn PatternLocale,
    pattern: &[u8],
    flags: u32,
    out: &mut GlobState,
) -> i32 {
    raven_libc_glob::glob(fs, env, loc, pattern, flags, None, out)
}

pub fn paths(out: &GlobState) -> Vec<String> {
    out.pathv
        .iter()
        .map(|p| String::from_utf8_lossy(p).into_owned())
        .collect()
}

/// `Errno(ENOSYS)` for tests that need a never-used environment.
pub const NEVER: i32 = ENOSYS;
