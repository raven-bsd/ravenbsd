// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! The real Linux backend: `openat`/`fstatat`/`fstat`/`getdents64`/`lseek`/`close` through
//! `extern "C"`, against a temporary directory the test creates itself.
//!
//! This is the "host-Linux backend test" of the ticket: it is the only place that touches a
//! real filesystem, and it compares the real walk with the fake tree for the same shape.

use std::ffi::{c_char, c_int, c_long, c_void};
use std::fs;
use std::os::unix::fs::{PermissionsExt, symlink};
use std::path::PathBuf;

use raven_libc_abi::errno::Errno;
use raven_libc_abi::generated::errno::ENOENT;
use raven_libc_abi::generated::structs::{Stat, Timespec};
use raven_libc_fts::scandir::{alphasort, scandir, versionsort};
use raven_libc_fts::{
    AT_FDCWD, DirStream, DirSys, Fd, Fts, Whence, check_cookie_contract, ftw, nftw,
};

#[repr(C)]
#[derive(Clone, Copy)]
struct LinuxTimespec {
    tv_sec: i64,
    tv_nsec: i64,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct LinuxStat {
    st_dev: u64,
    st_ino: u64,
    st_nlink: u64,
    st_mode: u32,
    st_uid: u32,
    st_gid: u32,
    __pad0: u32,
    st_rdev: u64,
    st_size: i64,
    st_blksize: i64,
    st_blocks: i64,
    st_atim: LinuxTimespec,
    st_mtim: LinuxTimespec,
    st_ctim: LinuxTimespec,
    __unused: [i64; 3],
}

impl LinuxStat {
    const ZERO: Self = Self {
        st_dev: 0,
        st_ino: 0,
        st_nlink: 0,
        st_mode: 0,
        st_uid: 0,
        st_gid: 0,
        __pad0: 0,
        st_rdev: 0,
        st_size: 0,
        st_blksize: 0,
        st_blocks: 0,
        st_atim: LinuxTimespec {
            tv_sec: 0,
            tv_nsec: 0,
        },
        st_mtim: LinuxTimespec {
            tv_sec: 0,
            tv_nsec: 0,
        },
        st_ctim: LinuxTimespec {
            tv_sec: 0,
            tv_nsec: 0,
        },
        __unused: [0; 3],
    };

    fn as_abi(self) -> Stat {
        Stat {
            st_dev: self.st_dev,
            st_ino: self.st_ino,
            st_mode: self.st_mode,
            st_nlink: self.st_nlink as u32,
            st_uid: self.st_uid,
            st_gid: self.st_gid,
            st_rdev: self.st_rdev,
            __pad1: 0,
            st_size: self.st_size,
            st_blksize: self.st_blksize as i32,
            __pad2: 0,
            st_blocks: self.st_blocks,
            st_atim: Timespec {
                tv_sec: self.st_atim.tv_sec,
                tv_nsec: self.st_atim.tv_nsec,
            },
            st_mtim: Timespec {
                tv_sec: self.st_mtim.tv_sec,
                tv_nsec: self.st_mtim.tv_nsec,
            },
            st_ctim: Timespec {
                tv_sec: self.st_ctim.tv_sec,
                tv_nsec: self.st_ctim.tv_nsec,
            },
            __unused: [0; 2],
        }
    }
}

unsafe extern "C" {
    fn openat(dirfd: c_int, path: *const c_char, flags: c_int, mode: c_int) -> c_int;
    fn fstatat(dirfd: c_int, path: *const c_char, stat: *mut LinuxStat, flags: c_int) -> c_int;
    fn fstat(fd: c_int, stat: *mut LinuxStat) -> c_int;
    fn getdents64(fd: c_int, buf: *mut c_void, len: usize) -> c_long;
    fn lseek(fd: c_int, off: i64, whence: c_int) -> i64;
    fn close(fd: c_int) -> c_int;
    fn __errno_location() -> *mut c_int;
}

const AT_SYMLINK_NOFOLLOW: c_int = 0x100;
const SEEK_SET: c_int = 0;
const SEEK_CUR: c_int = 1;
const SEEK_END: c_int = 2;

fn errno() -> Errno {
    // SAFETY: the libc errno accessor returns a valid thread-local pointer.
    unsafe { Errno(*__errno_location() as u16) }
}

struct HostDirSys;

fn cpath(path: &[u8]) -> Vec<u8> {
    let mut bytes = path.to_vec();
    bytes.push(0);
    bytes
}

impl DirSys for HostDirSys {
    fn openat(&mut self, dirfd: i32, name: &[u8], flags: u32) -> Result<Fd, Errno> {
        let path = cpath(name);
        // SAFETY: `path` is NUL-terminated and lives for the call; the flags are plain integers.
        let fd = unsafe { openat(dirfd, path.as_ptr().cast::<c_char>(), flags as c_int, 0) };
        if fd < 0 {
            return Err(errno());
        }
        Ok(Fd(fd))
    }

    fn fstatat(&mut self, dirfd: i32, name: &[u8], nofollow: bool) -> Result<Stat, Errno> {
        let path = cpath(name);
        let mut stat = LinuxStat::ZERO;
        let flags = if nofollow { AT_SYMLINK_NOFOLLOW } else { 0 };
        // SAFETY: `path` is NUL-terminated, `stat` is a valid out-pointer for the syscall.
        let rc = unsafe { fstatat(dirfd, path.as_ptr().cast::<c_char>(), &mut stat, flags) };
        if rc != 0 {
            return Err(errno());
        }
        Ok(stat.as_abi())
    }

    fn fstat(&mut self, fd: Fd) -> Result<Stat, Errno> {
        let mut stat = LinuxStat::ZERO;
        // SAFETY: `stat` is a valid out-pointer for the syscall.
        let rc = unsafe { fstat(fd.0, &mut stat) };
        if rc != 0 {
            return Err(errno());
        }
        Ok(stat.as_abi())
    }

    fn getdents(&mut self, fd: Fd, buf: &mut [u8]) -> Result<usize, Errno> {
        // SAFETY: the buffer is valid for `buf.len()` bytes and the fd is owned by the stream.
        let rc = unsafe { getdents64(fd.0, buf.as_mut_ptr().cast::<c_void>(), buf.len()) };
        if rc < 0 {
            return Err(errno());
        }
        usize::try_from(rc).map_err(|_| Errno(ENOENT as u16))
    }

    fn lseek(&mut self, fd: Fd, off: i64, whence: Whence) -> Result<i64, Errno> {
        let mode = match whence {
            Whence::Set => SEEK_SET,
            Whence::Cur => SEEK_CUR,
            Whence::End => SEEK_END,
        };
        // SAFETY: plain integer syscall.
        let rc = unsafe { lseek(fd.0, off, mode) };
        if rc < 0 {
            return Err(errno());
        }
        Ok(rc)
    }

    fn close(&mut self, fd: Fd) -> Result<(), Errno> {
        // SAFETY: plain integer syscall.
        let rc = unsafe { close(fd.0) };
        if rc != 0 {
            return Err(errno());
        }
        Ok(())
    }
}

struct Tree(PathBuf);

impl Tree {
    fn new(name: &str) -> Self {
        let root =
            std::env::temp_dir().join(format!("raven-c46-fts-{}-{name}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        Self(root)
    }

    fn path(&self, rel: &str) -> PathBuf {
        self.0.join(rel)
    }
}

impl Drop for Tree {
    fn drop(&mut self) {
        let _ = fs::set_permissions(&self.0, fs::Permissions::from_mode(0o755));
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn collect(fts: &mut Fts, sys: &mut HostDirSys) -> Vec<(i32, Vec<u8>, i64)> {
    let mut out = Vec::new();
    while let Some(node) = fts.read(sys).unwrap() {
        out.push((node.info, node.path.clone(), node.level));
    }
    out
}

#[test]
fn real_opendir_readdir_and_cookie_contract() {
    let tree = Tree::new("dirent");
    fs::create_dir(tree.path("d")).unwrap();
    for i in 0..5000 {
        fs::write(tree.path(&format!("d/f{i}")), b"").unwrap();
    }
    let mut sys = HostDirSys;
    check_cookie_contract(&mut sys, tree.path("d").to_str().unwrap().as_bytes()).unwrap();
    let mut dir =
        DirStream::opendir(&mut sys, tree.path("d").to_str().unwrap().as_bytes()).unwrap();
    let mut count = 0;
    while dir.readdir(&mut sys).unwrap().is_some() {
        count += 1;
    }
    assert_eq!(count, 5002);
    dir.rewinddir(&mut sys);
    assert_eq!(dir.readdir(&mut sys).unwrap().unwrap().name, b".");
    dir.closedir(&mut sys).unwrap();
}

#[test]
fn real_scandir_sorts() {
    let tree = Tree::new("scandir");
    fs::create_dir(tree.path("d")).unwrap();
    for name in ["v10", "v2", "a", "C"] {
        fs::write(tree.path(&format!("d/{name}")), b"").unwrap();
    }
    let path = tree.path("d");
    let mut sys = HostDirSys;
    let entries = scandir(
        &mut sys,
        path.to_str().unwrap().as_bytes(),
        None,
        Some(&mut alphasort),
    )
    .unwrap();
    let names: Vec<String> = entries
        .iter()
        .map(|entry| String::from_utf8_lossy(&entry.name).into_owned())
        .collect();
    assert_eq!(names, vec![".", "..", "C", "a", "v10", "v2"]);
    let entries = scandir(
        &mut sys,
        path.to_str().unwrap().as_bytes(),
        None,
        Some(&mut versionsort),
    )
    .unwrap();
    let names: Vec<String> = entries
        .iter()
        .map(|entry| String::from_utf8_lossy(&entry.name).into_owned())
        .collect();
    assert_eq!(names, vec![".", "..", "C", "a", "v2", "v10"]);
}

#[test]
fn real_fts_walk_matches_the_expected_sequence() {
    let tree = Tree::new("fts");
    fs::create_dir(tree.path("r")).unwrap();
    fs::write(tree.path("r/a"), b"x").unwrap();
    fs::create_dir(tree.path("r/sub")).unwrap();
    fs::write(tree.path("r/sub/x"), b"x").unwrap();
    let mut sys = HostDirSys;
    let root = tree.path("r");
    let mut fts = Fts::open(
        &mut sys,
        AT_FDCWD,
        &[root.to_str().unwrap().as_bytes()],
        raven_libc_fts::FTS_PHYSICAL,
        None,
    )
    .unwrap();
    let nodes = collect(&mut fts, &mut sys);
    fts.close(&mut sys).unwrap();
    let infos: Vec<i32> = nodes.iter().map(|(info, _, _)| *info).collect();
    assert_eq!(
        infos,
        vec![
            raven_libc_fts::FTS_D,
            raven_libc_fts::FTS_F,
            raven_libc_fts::FTS_D,
            raven_libc_fts::FTS_F,
            raven_libc_fts::FTS_DP,
            raven_libc_fts::FTS_DP
        ]
    );
    assert_eq!(nodes[0].2, 0);
    assert_eq!(nodes[3].2, 2);
}

#[test]
fn real_physical_walk_does_not_follow_a_symlink() {
    let tree = Tree::new("symlink");
    fs::create_dir(tree.path("r")).unwrap();
    fs::create_dir(tree.path("r/target")).unwrap();
    fs::write(tree.path("r/target/inside"), b"x").unwrap();
    symlink("target", tree.path("r/link")).unwrap();
    let mut sys = HostDirSys;
    let root = tree.path("r");
    let mut fts = Fts::open(
        &mut sys,
        AT_FDCWD,
        &[root.to_str().unwrap().as_bytes()],
        raven_libc_fts::FTS_PHYSICAL,
        None,
    )
    .unwrap();
    let nodes = collect(&mut fts, &mut sys);
    fts.close(&mut sys).unwrap();
    let link = nodes
        .iter()
        .find(|(_, path, _)| path.ends_with(b"link"))
        .unwrap();
    assert_eq!(link.0, raven_libc_fts::FTS_SL);
    assert!(
        !nodes
            .iter()
            .any(|(_, path, _)| path.ends_with(b"link/inside"))
    );
}

#[test]
fn real_logical_walk_follows_and_detects_a_cycle() {
    let tree = Tree::new("cycle");
    fs::create_dir(tree.path("r")).unwrap();
    fs::create_dir(tree.path("r/a")).unwrap();
    fs::write(tree.path("r/a/f"), b"x").unwrap();
    symlink("..", tree.path("r/a/up")).unwrap();
    let mut sys = HostDirSys;
    let root = tree.path("r");
    let mut fts = Fts::open(
        &mut sys,
        AT_FDCWD,
        &[root.to_str().unwrap().as_bytes()],
        raven_libc_fts::FTS_LOGICAL,
        None,
    )
    .unwrap();
    let nodes = collect(&mut fts, &mut sys);
    fts.close(&mut sys).unwrap();
    let up = nodes
        .iter()
        .find(|(_, path, _)| path.ends_with(b"up"))
        .unwrap();
    assert_eq!(up.0, raven_libc_fts::FTS_DC);
    let a_visits = nodes
        .iter()
        .filter(|(info, path, _)| *info == raven_libc_fts::FTS_D && path.ends_with(b"a"))
        .count();
    assert_eq!(a_visits, 1);
}

#[test]
fn real_unreadable_directory_is_dnr() {
    let tree = Tree::new("dnr");
    fs::create_dir(tree.path("r")).unwrap();
    fs::create_dir(tree.path("r/locked")).unwrap();
    fs::set_permissions(tree.path("r/locked"), fs::Permissions::from_mode(0o000)).unwrap();
    let mut sys = HostDirSys;
    let root = tree.path("r");
    let mut fts = Fts::open(
        &mut sys,
        AT_FDCWD,
        &[root.to_str().unwrap().as_bytes()],
        raven_libc_fts::FTS_PHYSICAL,
        None,
    )
    .unwrap();
    let nodes = collect(&mut fts, &mut sys);
    fts.close(&mut sys).unwrap();
    fs::set_permissions(tree.path("r/locked"), fs::Permissions::from_mode(0o755)).unwrap();
    let locked = nodes
        .iter()
        .rev()
        .find(|(_, path, _)| path.ends_with(b"locked"))
        .unwrap();
    assert_eq!(locked.0, raven_libc_fts::FTS_DNR);
}

#[test]
fn real_ftw_and_nftw() {
    let tree = Tree::new("ftw");
    fs::create_dir(tree.path("r")).unwrap();
    fs::write(tree.path("r/a"), b"x").unwrap();
    fs::create_dir(tree.path("r/sub")).unwrap();
    fs::write(tree.path("r/sub/x"), b"x").unwrap();
    let root = tree.path("r");
    let root = root.to_str().unwrap().as_bytes();
    let mut sys = HostDirSys;
    let mut seen: Vec<(Vec<u8>, i32)> = Vec::new();
    let mut callback = |path: &[u8], _stat: &Stat, flag: i32| {
        seen.push((path.to_vec(), flag));
        0
    };
    assert_eq!(ftw(&mut sys, root, &mut callback, 1).unwrap(), 0);
    assert!(
        seen.iter()
            .any(|(path, flag)| path.ends_with(b"sub/x") && *flag == raven_libc_fts::FTW_F)
    );

    let mut post: Vec<(Vec<u8>, i32)> = Vec::new();
    let mut nftw_callback = |path: &[u8], _stat: &Stat, flag: i32, _ftw: &raven_libc_fts::Ftw| {
        post.push((path.to_vec(), flag));
        0
    };
    assert_eq!(
        nftw(
            &mut sys,
            root,
            &mut nftw_callback,
            1,
            raven_libc_fts::FTW_DEPTH
        )
        .unwrap(),
        0
    );
    let last = post.last().unwrap();
    assert_eq!(last.1, raven_libc_fts::FTW_DP);
    assert!(last.0.ends_with(b"r"));
}

#[test]
fn real_255_byte_name() {
    let tree = Tree::new("name");
    fs::create_dir(tree.path("r")).unwrap();
    let name = "n".repeat(255);
    fs::write(tree.path(&format!("r/{name}")), b"").unwrap();
    let mut sys = HostDirSys;
    let root = tree.path("r");
    let mut fts = Fts::open(
        &mut sys,
        AT_FDCWD,
        &[root.to_str().unwrap().as_bytes()],
        raven_libc_fts::FTS_PHYSICAL,
        None,
    )
    .unwrap();
    let nodes = collect(&mut fts, &mut sys);
    fts.close(&mut sys).unwrap();
    assert!(
        nodes
            .iter()
            .any(|(_, path, _)| path.ends_with(name.as_bytes()))
    );
}

#[test]
fn real_fts_openat_relative_roots() {
    let tree = Tree::new("openat");
    fs::create_dir(tree.path("base")).unwrap();
    fs::create_dir(tree.path("base/d")).unwrap();
    fs::write(tree.path("base/d/x"), b"x").unwrap();
    let mut sys = HostDirSys;
    let base = tree.path("base");
    let base = base.to_str().unwrap().as_bytes();
    let base_fd = sys
        .openat(
            AT_FDCWD,
            base,
            raven_libc_abi::generated::fcntl::O_DIRECTORY as u32,
        )
        .unwrap();
    let mut fts = Fts::open(
        &mut sys,
        base_fd.0,
        &[b"d"],
        raven_libc_fts::FTS_PHYSICAL,
        None,
    )
    .unwrap();
    let nodes = collect(&mut fts, &mut sys);
    fts.close(&mut sys).unwrap();
    sys.close(base_fd).unwrap();
    assert_eq!(nodes[0].1, b"d");
    assert!(nodes.iter().any(|(_, path, _)| path == b"d/x"));
}

#[test]
fn real_ephemeral_directory_is_eio_free() {
    // Open a directory, remove it, and keep reading: Linux returns ENOENT on the next getdents,
    // which the stream maps to end-of-stream (A-dir-1), not to an error.
    let tree = Tree::new("ephemeral");
    fs::create_dir(tree.path("d")).unwrap();
    fs::write(tree.path("d/a"), b"x").unwrap();
    let mut sys = HostDirSys;
    let path = tree.path("d");
    let mut dir = DirStream::opendir(&mut sys, path.to_str().unwrap().as_bytes()).unwrap();
    assert!(dir.readdir(&mut sys).unwrap().is_some());
    fs::remove_file(tree.path("d/a")).unwrap();
    fs::remove_dir(tree.path("d")).unwrap();
    // The directory entry still exists for the open handle (Linux keeps it until close), so the
    // stream may legitimately continue; the contract is that a getdents ENOENT is never an error.
    loop {
        match dir.readdir(&mut sys) {
            Ok(Some(_)) => {}
            Ok(None) => break,
            Err(error) => panic!("getdents error {error:?}"),
        }
    }
    dir.closedir(&mut sys).unwrap();
}
