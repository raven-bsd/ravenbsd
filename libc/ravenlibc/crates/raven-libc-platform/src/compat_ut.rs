// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `compat` alias tests.

use raven_libc_abi::fcntl;

use crate::backend::FakeSys;
use crate::compat::{
    close_range, closefrom, getrlimit, getrlimit64, open64, posix_close, prlimit, setrlimit, umask,
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
