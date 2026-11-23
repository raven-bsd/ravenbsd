// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `compat` alias tests.

use raven_libc_abi::fcntl;

use crate::backend::FakeSys;
use crate::compat::{
    close_range, closefrom, getrlimit, getrlimit64, open64, posix_close, prlimit, setrlimit,
    setrlimit64, umask,
};
use crate::platform::Platform;

fn platform() -> Platform<FakeSys> {
    let fake = FakeSys::new();
    fake.file(b"/a", b"x");
    Platform::new(fake)
}

#[test]
fn open64_matches_open() {
    let platform = platform();
    let fd = open64(&platform, b"/a", fcntl::O_RDONLY as u32, 0).expect("open64");
    assert_eq!(fd, 0);
}

#[test]
fn posix_close_matches_close() {
    let platform = platform();
    let fd = open64(&platform, b"/a", fcntl::O_RDONLY as u32, 0).expect("open64");
    assert_eq!(posix_close(&platform, fd), Ok(()));
}

#[test]
fn close_range_and_closefrom() {
    let platform = platform();
    for _ in 0..4 {
        open64(&platform, b"/a", fcntl::O_RDONLY as u32, 0).expect("open");
    }
    close_range(&platform, 0, 2).expect("range");
    assert_eq!(platform.open_count(), 2);
    closefrom(&platform, 0).expect("from");
    assert_eq!(platform.open_count(), 0);
}

#[test]
fn umask_returns_the_previous_value() {
    let platform = platform();
    assert_eq!(umask(&platform, 0o077), 0o022);
    assert_eq!(umask(&platform, 0o022), 0o077);
}

#[test]
fn rlimits_round_trip() {
    let platform = platform();
    assert_eq!(getrlimit(&platform, 7), Ok((1024, 32768)));
    setrlimit(&platform, 7, 256, 512).expect("setrlimit");
    assert_eq!(getrlimit64(&platform, 7), Ok((256, 512)));
    assert_eq!(prlimit(&platform, 0, 7, None), Ok((256, 512)));
}

#[test]
fn setrlimit64_matches_setrlimit() {
    let platform = platform();
    setrlimit64(&platform, 7, 128, 256).expect("setrlimit64");
    assert_eq!(getrlimit64(&platform, 7), Ok((128, 256)));
    assert_eq!(
        setrlimit64(&platform, 7, 256, 128),
        Err(crate::Errno::EINVAL)
    );
}

#[test]
fn invalid_rlimit_is_einval() {
    let platform = platform();
    assert_eq!(getrlimit(&platform, 1), Err(crate::Errno::EINVAL));
    assert_eq!(setrlimit(&platform, 7, 10, 5), Err(crate::Errno::EINVAL));
}

#[test]
fn ttyname_rejects_a_file() {
    let platform = platform();
    let fd = open64(&platform, b"/a", fcntl::O_RDONLY as u32, 0).expect("open64");
    assert_eq!(
        crate::compat::ttyname(&platform, fd),
        Err(crate::Errno::ENOTTY)
    );
}

#[test]
fn ttyname_names_a_pathless_pty_slave_from_the_server_inode() {
    // The spawned-child shape: the fd has the pty handle but no public path. The pty server's
    // FSTAT inode (slave = 2*pair + 2) is the pts name. The fake FSTAT inode is handle + 100.
    let fake = FakeSys::new();
    fake.tty(b"/dev/tty");
    let platform = Platform::new(fake);
    let first = open64(&platform, b"/dev/tty", fcntl::O_RDWR as u32, 0).expect("first");
    let second = open64(&platform, b"/dev/tty", fcntl::O_RDWR as u32, 0).expect("second");
    let handle = platform.get(second).expect("fd").handle();
    let fd = platform
        .install(
            handle,
            crate::OfdKind::Tty,
            fcntl::O_RDWR as u32,
            None,
            false,
        )
        .expect("install");
    // The second open's fake handle is 6, so FSTAT ino = 106 and pair = 52.
    assert_eq!(
        crate::compat::ttyname(&platform, fd),
        Ok(b"/dev/pts/52".to_vec())
    );
    let _ = platform.close(fd);
    let _ = platform.close(first);
    let _ = platform.close(second);
}
