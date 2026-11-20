// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! P1c unistd/IO wrapper tests.

use super::*;
use crate::backend::FakeSys;

fn platform() -> Platform<FakeSys> {
    let fake = FakeSys::new();
    fake.dir(b"/");
    fake.file(b"/a", b"x");
    fake.sys(b"kern/hostid", b"1234\n");
    fake.sys(b"kern/domainname", b"example\n");
    Platform::new(fake)
}

#[test]
fn chown_family_uses_the_root_handle() {
    let platform = platform();
    assert_eq!(platform.chown(b"/a", 1, 2), Ok(()));
    assert_eq!(platform.lchown(b"/a", 1, 2), Ok(()));
    let fd = platform.open(b"/a", 0, 0).expect("open");
    assert_eq!(platform.fchown(fd, 1, 2), Ok(()));
    let _ = platform.close(fd);
}

#[test]
fn mkfifoat_and_mknodat_resolve_through_the_cwd() {
    let platform = platform();
    assert_eq!(platform.mkfifoat(-100, b"fifo", 0o644), Ok(()));
    assert_eq!(platform.mknodat(-100, b"node", 0o600, 7), Ok(()));
}

#[test]
fn utime_sends_the_explicit_times() {
    let platform = platform();
    assert_eq!(platform.utime(b"/a", Some(10), Some(20)), Ok(()));
    assert_eq!(platform.utime(b"/a", None, Some(20)), Ok(()));
}

#[test]
fn getdtablesize_tracks_the_nofile_limit() {
    let platform = platform();
    assert_eq!(platform.getdtablesize(), 1024);
    platform.set_nofile_limits(256, 512);
    assert_eq!(platform.getdtablesize(), 256);
}

#[test]
fn exec_fdmap_env_carries_the_table_and_closes_cloexec() {
    use crate::fd::inherit::{ENV_NAME, FdMap};

    let platform = platform();
    let plain = platform.open(b"/a", 0, 0).expect("open");
    let cloexec = platform.open(b"/a", 0, 0).expect("open");
    platform.set_cloexec(cloexec, true).expect("cloexec");

    let entry = platform.exec_fdmap_env().expect("env").expect("some");
    let (name, value) = entry.split_at(ENV_NAME.len());
    assert_eq!(name, ENV_NAME);
    let value = value.strip_prefix(b"=").expect("separator");
    let map = FdMap::decode_env(value).expect("decode");
    assert!(map.entries.iter().any(|row| row.fd == plain));
    assert!(
        map.entries
            .iter()
            .any(|row| row.fd == cloexec && row.fd_flags & 1 != 0)
    );

    platform.close_cloexec_fds().expect("close");
    assert!(platform.get(cloexec).is_err());
    assert!(platform.get(plain).is_ok());
}

#[test]
fn hostid_and_domainname_round_trip() {
    let platform = platform();
    assert_eq!(platform.gethostid(), 1234);
    let mut buffer = [0u8; 32];
    assert_eq!(platform.getdomainname(&mut buffer), Ok(()));
    assert_eq!(&buffer[..11], b"example\0\0\0\0");
    assert_eq!(platform.setdomainname(b"other"), Ok(()));
    let mut buffer = [0u8; 32];
    assert_eq!(platform.getdomainname(&mut buffer), Ok(()));
    assert_eq!(&buffer[..6], b"other\0");
}

#[test]
fn lseek_moves_the_fake_cursor() {
    let platform = platform();
    let fd = platform.open(b"/a", 0, 0).expect("open");
    assert_eq!(platform.lseek(fd, 0, 2), Ok(1));
    assert_eq!(platform.lseek(fd, -1, 1), Ok(0));
    assert_eq!(platform.lseek(fd, 3, 0), Ok(3));
    assert_eq!(platform.lseek(fd, 0, 1), Ok(3));
    assert_eq!(platform.lseek(fd, 0, 9), Err(Errno::EINVAL));
    assert_eq!(platform.lseek(99, 0, 0), Err(Errno::EBADF));
    let _ = platform.close(fd);
}
