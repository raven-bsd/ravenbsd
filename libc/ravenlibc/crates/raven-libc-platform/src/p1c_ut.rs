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

#[test]
fn a_blocking_socket_read_waits_and_retries_after_eagain() {
    let platform = platform();
    let fd = platform
        .open(b"/a", fcntl::O_RDONLY as u32, 0)
        .expect("open");
    let index = platform.get(fd).expect("fd").index();
    platform.set_ofd_kind(index, OfdKind::Socket);
    platform
        .set_ofd_event(index, crate::Handle(9))
        .expect("event");
    platform
        .backend()
        .ready(crate::Handle(9), crate::poll::EV_SIGNALED);
    platform
        .backend()
        .fail_next(crate::backend::Fault::Read, Errno::EAGAIN);
    let mut buffer = [0u8; 4];
    assert_eq!(platform.read(fd, &mut buffer), Ok(1));
    assert_eq!(buffer[0], b'x');
    let _ = platform.close(fd);
}

#[test]
fn a_nonblocking_socket_read_returns_eagain() {
    let platform = platform();
    let fd = platform
        .open(b"/a", fcntl::O_RDONLY as u32, 0)
        .expect("open");
    let index = platform.get(fd).expect("fd").index();
    platform.set_ofd_kind(index, OfdKind::Socket);
    platform.set_ofd_status(index, fcntl::O_NONBLOCK as u32);
    platform
        .backend()
        .fail_next(crate::backend::Fault::Read, Errno::EAGAIN);
    let mut buffer = [0u8; 4];
    assert_eq!(platform.read(fd, &mut buffer), Err(Errno::EAGAIN));
    let _ = platform.close(fd);
}

#[test]
fn closing_an_fd_releases_its_doorbell_with_handle_close() {
    let platform = platform();
    let base = platform.backend().open_handles();
    let fd = platform.open(b"/a", 0, 0).expect("open");
    let doorbell = platform
        .backend()
        .dup(platform.get(fd).expect("fd").handle(), 0)
        .expect("a second handle stands in for the doorbell");
    let index = platform.get(fd).expect("fd").index();
    platform.set_ofd_event(index, doorbell).expect("attach");
    assert_eq!(
        platform.backend().open_handles(),
        base + 2,
        "data + doorbell"
    );
    assert_eq!(platform.close(fd), Ok(()));
    assert_eq!(
        platform.backend().handle_close_calls(),
        1,
        "the doorbell goes through HANDLE_CLOSE"
    );
    assert_eq!(platform.backend().open_handles(), base, "nothing leaks");
}
