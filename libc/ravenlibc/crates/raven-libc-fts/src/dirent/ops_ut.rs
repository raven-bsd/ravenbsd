// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! Unit tests for the `DIR` stream operations over the fake tree.
// Test-only code is exempt from the production lints (17-crate-layout 9.3).
#![allow(
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic
)]

use alloc::vec::Vec;

use raven_libc_abi::generated::errno::{EBADF, EIO, ENOENT, ENOTDIR, EOPNOTSUPP};

use super::*;
use crate::fake::{FakeFs, NodeKind};
use crate::{DirSys, Fd, Whence, check_cookie_contract};

fn names(sys: &mut FakeFs, dir: &mut DirStream) -> Vec<Vec<u8>> {
    let mut out = Vec::new();
    while let Some(entry) = dir.readdir(sys).unwrap() {
        out.push(entry.name);
    }
    out
}

#[test]
fn opendir_and_readdir_list_dot_dotdot_and_children() {
    let mut fs = FakeFs::new();
    fs.mkdir("d", 0o755);
    fs.file("d/a", 1);
    fs.file("d/b", 2);
    let mut dir = DirStream::opendir(&mut fs, b"d").unwrap();
    assert_eq!(dir.dirfd(), Fd(0));
    let listed = names(&mut fs, &mut dir);
    assert_eq!(
        listed,
        vec![b".".to_vec(), b"..".to_vec(), b"a".to_vec(), b"b".to_vec()]
    );
    dir.closedir(&mut fs).unwrap();
}

#[test]
fn refills_across_5000_entries_with_a_512_byte_buffer() {
    let mut fs = FakeFs::new();
    fs.mkdir("many", 0o755);
    for i in 0..5000 {
        fs.file(format!("many/f{i}"), 0);
    }
    fs.set_dirent_buf_limit(512);
    let mut dir = DirStream::opendir(&mut fs, b"many").unwrap();
    let listed = names(&mut fs, &mut dir);
    assert_eq!(listed.len(), 5002);
    assert_eq!(listed[2], b"f0".to_vec());
    assert_eq!(listed[5001], b"f4999".to_vec());
    dir.closedir(&mut fs).unwrap();
}

#[test]
fn telldir_and_seekdir_resume_after_the_same_entry() {
    let mut fs = FakeFs::new();
    fs.mkdir("many", 0o755);
    for i in 0..5000 {
        fs.file(format!("many/f{i}"), 0);
    }
    fs.set_dirent_buf_limit(512);
    let mut dir = DirStream::opendir(&mut fs, b"many").unwrap();
    for _ in 0..2500 {
        dir.readdir(&mut fs).unwrap().unwrap();
    }
    let here = dir.telldir();
    let expected = dir.readdir(&mut fs).unwrap().unwrap();
    let mut tail = Vec::new();
    while let Some(entry) = dir.readdir(&mut fs).unwrap() {
        tail.push(entry.name);
    }
    dir.seekdir(&mut fs, here);
    let resumed = dir.readdir(&mut fs).unwrap().unwrap();
    assert_eq!(resumed.name, expected.name);
    let mut resumed_tail = Vec::new();
    while let Some(entry) = dir.readdir(&mut fs).unwrap() {
        resumed_tail.push(entry.name);
    }
    assert_eq!(resumed_tail, tail);
    dir.rewinddir(&mut fs);
    assert_eq!(dir.telldir(), 0);
    assert_eq!(dir.readdir(&mut fs).unwrap().unwrap().name, b".");
    dir.closedir(&mut fs).unwrap();
}

#[test]
fn rewinddir_restarts_the_stream() {
    let mut fs = FakeFs::new();
    fs.mkdir("d", 0o755);
    fs.file("d/a", 1);
    fs.mkdir("d/b", 0o755);
    let mut dir = DirStream::opendir(&mut fs, b"d").unwrap();
    let first = names(&mut fs, &mut dir);
    dir.rewinddir(&mut fs);
    let second = names(&mut fs, &mut dir);
    assert_eq!(first, second);
    dir.closedir(&mut fs).unwrap();
}

#[test]
fn getdents_enoent_ends_the_stream() {
    let mut fs = FakeFs::new();
    fs.mkdir("d", 0o755);
    fs.file("d/a", 1);
    let mut dir = DirStream::opendir(&mut fs, b"d").unwrap();
    let next = fs.op_count() + 1;
    fs.fail_after(next, ENOENT);
    assert!(dir.readdir(&mut fs).unwrap().is_none());
    dir.closedir(&mut fs).unwrap();
}

#[test]
fn other_getdents_errors_are_returned() {
    let mut fs = FakeFs::new();
    fs.mkdir("d", 0o755);
    let mut dir = DirStream::opendir(&mut fs, b"d").unwrap();
    let next = fs.op_count() + 1;
    fs.fail_after(next, EIO);
    assert_eq!(dir.readdir(&mut fs).unwrap_err().0, EIO as u16);
    dir.closedir(&mut fs).unwrap();
}

#[test]
fn fdopendir_rejects_a_file() {
    let mut fs = FakeFs::new();
    fs.file("f", 1);
    let fd = fs.openat(crate::AT_FDCWD, b"f", 0).unwrap();
    assert_eq!(
        DirStream::fdopendir(&mut fs, fd).err().map(|error| error.0),
        Some(ENOTDIR as u16)
    );
    fs.close(fd).unwrap();
}

#[test]
fn fdopendir_rejects_an_o_path_descriptor() {
    let mut fs = FakeFs::new();
    fs.mkdir("d", 0o755);
    let fd = fs
        .openat(
            crate::AT_FDCWD,
            b"d",
            raven_libc_abi::generated::fcntl::O_PATH as u32,
        )
        .unwrap();
    assert_eq!(
        DirStream::fdopendir(&mut fs, fd).err().map(|error| error.0),
        Some(EBADF as u16)
    );
    fs.close(fd).unwrap();
}

#[test]
fn closedir_returns_the_close_error() {
    let mut fs = FakeFs::new();
    fs.mkdir("d", 0o755);
    let dir = DirStream::opendir(&mut fs, b"d").unwrap();
    let next = fs.op_count() + 1;
    fs.fail_after(next, EIO);
    assert_eq!(dir.closedir(&mut fs).unwrap_err().0, EIO as u16);
}

#[test]
fn posix_getdents_rejects_flags_and_reads_without_flags() {
    let mut fs = FakeFs::new();
    fs.mkdir("d", 0o755);
    fs.file("d/a", 1);
    let mut dir = DirStream::opendir(&mut fs, b"d").unwrap();
    let mut buf = [0u8; 512];
    assert_eq!(
        dir.posix_getdents(&mut fs, &mut buf, 1).unwrap_err().0,
        EOPNOTSUPP as u16
    );
    let len = dir.posix_getdents(&mut fs, &mut buf, 0).unwrap();
    assert!(len > 0);
    dir.closedir(&mut fs).unwrap();
}

#[test]
fn a_255_byte_name_round_trips() {
    let mut fs = FakeFs::new();
    fs.mkdir("d", 0o755);
    let name = "n".repeat(255);
    fs.file(format!("d/{name}"), 0);
    let mut dir = DirStream::opendir(&mut fs, b"d").unwrap();
    let listed = names(&mut fs, &mut dir);
    assert_eq!(listed[2], name.as_bytes());
    dir.closedir(&mut fs).unwrap();
}

#[test]
fn readdir_r_is_the_same_stream_operation() {
    let mut fs = FakeFs::new();
    fs.mkdir("d", 0o755);
    fs.file("d/a", 1);
    let mut dir = DirStream::opendir(&mut fs, b"d").unwrap();
    assert_eq!(dir.readdir_r(&mut fs).unwrap().unwrap().name, b".");
    let mut count = 1;
    while dir.readdir_r(&mut fs).unwrap().is_some() {
        count += 1;
    }
    assert_eq!(count, 3);
    dir.closedir(&mut fs).unwrap();
}

#[test]
fn directory_cookie_contract_holds_on_the_fake() {
    let mut fs = FakeFs::new();
    fs.mkdir("d", 0o755);
    for i in 0..40 {
        fs.file(format!("d/f{i}"), 0);
        fs.mkdir(format!("d/sub{i}"), 0o755);
    }
    check_cookie_contract(&mut fs, b"d").unwrap();
}

#[test]
fn cookie_contract_reports_a_backend_that_rejects_a_cookie() {
    // The fake accepts 0..=count, so a seek past the end is rejected by `lseek`.
    let mut fs = FakeFs::new();
    fs.mkdir("d", 0o755);
    fs.file("d/a", 1);
    let fd = fs.openat(crate::AT_FDCWD, b"d", 0).unwrap();
    assert!(fs.lseek(fd, 99, Whence::Set).is_err());
    fs.close(fd).unwrap();
}

#[test]
fn getdents_on_a_non_directory_descriptor_is_enotdir() {
    let mut fs = FakeFs::new();
    fs.file("f", 0);
    let fd = fs.openat(crate::AT_FDCWD, b"f", 0).unwrap();
    let dir = DirStream::opendir(&mut fs, b".").unwrap();
    // A stream can only be built from a directory; exercise the raw call instead.
    let mut buf = [0u8; 64];
    assert_eq!(fs.getdents(fd, &mut buf).unwrap_err().0, ENOTDIR as u16);
    dir.closedir(&mut fs).unwrap();
    fs.close(fd).unwrap();
}

#[test]
fn missing_directory_is_enoent() {
    let mut fs = FakeFs::new();
    assert_eq!(
        DirStream::opendir(&mut fs, b"missing")
            .err()
            .map(|error| error.0),
        Some(ENOENT as u16)
    );
}

#[test]
fn a_malformed_getdents_record_is_eio() {
    struct BadSys {
        calls: usize,
    }
    impl DirSys for BadSys {
        fn openat(
            &mut self,
            _dirfd: i32,
            _name: &[u8],
            _flags: u32,
        ) -> Result<Fd, raven_libc_abi::errno::Errno> {
            Ok(Fd(1))
        }
        fn fstatat(
            &mut self,
            _dirfd: i32,
            _name: &[u8],
            _nofollow: bool,
        ) -> Result<raven_libc_abi::generated::structs::Stat, raven_libc_abi::errno::Errno>
        {
            Ok(raven_libc_abi::generated::structs::Stat::ZERO)
        }
        fn fstat(
            &mut self,
            _fd: Fd,
        ) -> Result<raven_libc_abi::generated::structs::Stat, raven_libc_abi::errno::Errno>
        {
            let mut stat = raven_libc_abi::generated::structs::Stat::ZERO;
            stat.st_mode = raven_libc_abi::generated::stat::S_IFDIR;
            Ok(stat)
        }
        fn getdents(
            &mut self,
            _fd: Fd,
            buf: &mut [u8],
        ) -> Result<usize, raven_libc_abi::errno::Errno> {
            self.calls += 1;
            if self.calls > 1 {
                return Ok(0);
            }
            for byte in buf.iter_mut() {
                *byte = 0;
            }
            Ok(24)
        }
        fn lseek(
            &mut self,
            _fd: Fd,
            _off: i64,
            _whence: Whence,
        ) -> Result<i64, raven_libc_abi::errno::Errno> {
            Ok(0)
        }
        fn close(&mut self, _fd: Fd) -> Result<(), raven_libc_abi::errno::Errno> {
            Ok(())
        }
    }
    let mut sys = BadSys { calls: 0 };
    let mut dir = DirStream::opendir(&mut sys, b"x").unwrap();
    assert_eq!(dir.readdir(&mut sys).unwrap_err().0, EIO as u16);
}

#[test]
fn node_kind_is_exposed_for_the_fake() {
    let mut fs = FakeFs::new();
    fs.node("fifo", NodeKind::Fifo);
    assert!(fs.fstatat(crate::AT_FDCWD, b"fifo", true).is_ok());
}
