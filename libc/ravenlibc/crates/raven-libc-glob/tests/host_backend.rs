// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `glob` against a real temporary directory through a `std::fs` `DirSys`.
//!
//! The crate's own test tree is created here (never read from the environment); the results
//! are compared with the path-based fake for the same shape.

mod common;

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use common::{Fake, FakeEnv, paths, run};
use raven_libc_abi::errno::Errno;
use raven_libc_abi::generated::errno::{EACCES, ELOOP, ENOENT, ENOTDIR};
use raven_libc_abi::generated::structs::Stat;
use raven_libc_glob::consts::*;
use raven_libc_glob::{DirHandle, DirSys, GlobState, Utf8Locale};

struct HostFs {
    root: PathBuf,
    dirs: Vec<(Vec<Vec<u8>>, usize)>,
    lstat_calls: usize,
    stat_calls: usize,
}

impl HostFs {
    fn new(root: &Path) -> Self {
        Self {
            root: root.to_path_buf(),
            dirs: Vec::new(),
            lstat_calls: 0,
            stat_calls: 0,
        }
    }

    fn full(&self, path: &[u8]) -> PathBuf {
        let rel = std::str::from_utf8(path).unwrap_or("");
        self.root.join(rel.trim_start_matches('/'))
    }

    fn stat_to_abi(meta: &fs::Metadata) -> Stat {
        use std::os::unix::fs::MetadataExt;
        Stat {
            st_dev: meta.dev(),
            st_ino: meta.ino(),
            st_mode: meta.mode(),
            st_nlink: meta.nlink() as u32,
            st_uid: meta.uid(),
            st_gid: meta.gid(),
            st_rdev: meta.rdev(),
            __pad1: 0,
            st_size: meta.size() as i64,
            st_blksize: meta.blksize() as i32,
            __pad2: 0,
            st_blocks: meta.blocks() as i64,
            st_atim: raven_libc_abi::generated::structs::Timespec {
                tv_sec: meta.atime(),
                tv_nsec: meta.atime_nsec(),
            },
            st_mtim: raven_libc_abi::generated::structs::Timespec {
                tv_sec: meta.mtime(),
                tv_nsec: meta.mtime_nsec(),
            },
            st_ctim: raven_libc_abi::generated::structs::Timespec {
                tv_sec: meta.ctime(),
                tv_nsec: meta.ctime_nsec(),
            },
            __unused: [0; 2],
        }
    }
}

impl DirSys for HostFs {
    fn opendir(&mut self, path: &[u8]) -> Result<DirHandle, Errno> {
        let full = self.full(path);
        let entries = match fs::read_dir(&full) {
            Ok(entries) => entries,
            Err(error) => {
                let errno = error.raw_os_error().unwrap_or(ENOENT);
                return Err(Errno(errno as u16));
            }
        };
        let mut names = Vec::new();
        for entry in entries {
            let Ok(entry) = entry else {
                return Err(Errno(ENOENT as u16));
            };
            names.push(entry.file_name().as_encoded_bytes().to_vec());
        }
        let handle = DirHandle(self.dirs.len());
        self.dirs.push((names, 0));
        Ok(handle)
    }

    fn readdir(&mut self, d: &mut DirHandle) -> Result<Option<Vec<u8>>, Errno> {
        let Some((names, pos)) = self.dirs.get_mut(d.0) else {
            return Err(Errno(ENOENT as u16));
        };
        let name = names.get(*pos).cloned();
        if name.is_some() {
            *pos += 1;
        }
        Ok(name)
    }

    fn closedir(&mut self, _d: DirHandle) {}

    fn lstat(&mut self, path: &[u8]) -> Result<Stat, Errno> {
        self.lstat_calls += 1;
        let full = self.full(path);
        match fs::symlink_metadata(&full) {
            Ok(meta) => Ok(Self::stat_to_abi(&meta)),
            Err(error) => Err(Errno(error.raw_os_error().unwrap_or(ENOENT) as u16)),
        }
    }

    fn stat(&mut self, path: &[u8]) -> Result<Stat, Errno> {
        self.stat_calls += 1;
        let full = self.full(path);
        match fs::metadata(&full) {
            Ok(meta) => Ok(Self::stat_to_abi(&meta)),
            Err(error) => {
                let code = error.raw_os_error().unwrap_or(ENOENT);
                Err(Errno(code as u16))
            }
        }
    }
}

struct Tree(PathBuf);

impl Tree {
    fn new(name: &str) -> Self {
        let root =
            std::env::temp_dir().join(format!("raven-c46-glob-{}-{name}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        Self(root)
    }
}

impl Drop for Tree {
    fn drop(&mut self) {
        let _ = fs::set_permissions(&self.0, fs::Permissions::from_mode(0o755));
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn glob_host(fs: &mut HostFs, pattern: &str, flags: u32) -> (i32, Vec<String>) {
    let env = FakeEnv::new();
    let mut out = GlobState::default();
    let rv = raven_libc_glob::glob(
        fs,
        &env,
        &Utf8Locale,
        pattern.as_bytes(),
        flags,
        None,
        &mut out,
    );
    (rv, paths(&out))
}

#[test]
fn real_tree_matches_the_fake() {
    let tree = Tree::new("match");
    let root = &tree.0;
    fs::create_dir(root.join("d")).unwrap();
    fs::create_dir(root.join("d/sub")).unwrap();
    for name in ["a", "b", "c"] {
        fs::write(root.join("d").join(name), b"x").unwrap();
    }
    fs::write(root.join("d/sub/x"), b"x").unwrap();
    fs::set_permissions(root.join("d"), fs::Permissions::from_mode(0o755)).unwrap();

    let mut host = HostFs::new(root);
    let mut fake = Fake::new();
    fake.mkdir("d", 0o755);
    fake.mkdir("d/sub", 0o755);
    fake.file("d/a", 1);
    fake.file("d/b", 1);
    fake.file("d/c", 1);
    fake.file("d/sub/x", 1);

    for pattern in ["d/*", "d/[ab]", "d/?", "d/*/x", "d/sub/*"] {
        let (host_rv, host_paths) = glob_host(&mut host, pattern, 0);
        let (fake_rv, fake_paths, _) = {
            let mut out = GlobState::default();
            let rv = run(&mut fake, &FakeEnv::new(), pattern.as_bytes(), 0, &mut out);
            (rv, paths(&out), out.errno)
        };
        assert_eq!(host_rv, fake_rv, "pattern {pattern}");
        assert_eq!(host_paths, fake_paths, "pattern {pattern}");
    }
}

#[test]
fn permission_denied_directory() {
    let tree = Tree::new("perm");
    let root = &tree.0;
    fs::create_dir(root.join("locked")).unwrap();
    fs::write(root.join("locked/x"), b"x").unwrap();
    fs::set_permissions(root.join("locked"), fs::Permissions::from_mode(0o000)).unwrap();

    let mut host = HostFs::new(root);
    let (rv, got) = glob_host(&mut host, "locked/*", 0);
    assert_eq!(rv, GLOB_NOMATCH);
    assert!(got.is_empty());
    let (rv, _got) = glob_host(&mut host, "locked/*", GLOB_ERR);
    assert_eq!(rv, GLOB_ABORTED);

    fs::set_permissions(root.join("locked"), fs::Permissions::from_mode(0o755)).unwrap();
}

#[test]
fn symlink_loop_is_an_error_not_a_match() {
    let tree = Tree::new("loop");
    let root = &tree.0;
    std::os::unix::fs::symlink("b", root.join("a")).unwrap();
    std::os::unix::fs::symlink("a", root.join("b")).unwrap();
    let mut host = HostFs::new(root);
    let (rv, got) = glob_host(&mut host, "a/*", GLOB_ERR);
    assert_eq!(rv, GLOB_ABORTED);
    assert!(got.is_empty());
}

#[test]
fn large_directory_walks_completely() {
    let tree = Tree::new("large");
    let root = &tree.0;
    fs::create_dir(root.join("many")).unwrap();
    for i in 0..5000 {
        fs::write(root.join("many").join(format!("f{i}")), b"").unwrap();
    }
    let mut host = HostFs::new(root);
    let (rv, got) = glob_host(&mut host, "many/f*", 0);
    assert_eq!(rv, 0);
    assert_eq!(got.len(), 5000);
    assert_eq!(got.first().map(String::as_str), Some("many/f0"));
    assert_eq!(got.last().map(String::as_str), Some("many/f999"));
}

#[test]
fn glob_limit_on_a_real_tree() {
    let tree = Tree::new("limit");
    let root = &tree.0;
    fs::create_dir(root.join("many")).unwrap();
    for i in 0..2000 {
        fs::write(root.join("many").join(format!("f{i}")), b"").unwrap();
    }
    let mut host = HostFs::new(root);
    let (rv, got) = glob_host(&mut host, "many/f*", GLOB_LIMIT);
    assert_eq!(rv, GLOB_NOSPACE);
    assert!(got.len() <= 1024);
}

#[test]
fn broken_symlink_is_returned_not_dereferenced() {
    let tree = Tree::new("broken");
    let root = &tree.0;
    std::os::unix::fs::symlink("missing", root.join("dangling")).unwrap();
    let mut host = HostFs::new(root);
    let (rv, got) = glob_host(&mut host, "dang*", GLOB_MARK);
    assert_eq!(rv, 0);
    assert_eq!(got, vec!["dangling"]);
}

#[test]
fn eacces_and_eloop_values_are_what_the_backend_reports() {
    // The fake reports the same errno values the host backend maps.
    assert_eq!(EACCES, 13);
    assert_eq!(ELOOP, 40);
    assert_eq!(ENOENT, 2);
    assert_eq!(ENOTDIR, 20);
}
