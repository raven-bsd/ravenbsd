// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `isatty` tests.

use raven_libc_abi::fcntl;

use crate::backend::FakeSys;
use crate::platform::Platform;
use crate::tty::isatty::isatty;

#[test]
fn tty_is_a_tty() {
    let fake = FakeSys::new();
    fake.tty(b"/dev/tty");
    let platform = Platform::new(fake);
    let fd = platform
        .open(b"/dev/tty", fcntl::O_RDWR as u32, 0)
        .expect("open");
    assert!(isatty(&platform, fd));
}

#[test]
fn regular_file_is_not_a_tty() {
    let fake = FakeSys::new();
    fake.file(b"/a", b"x");
    let platform = Platform::new(fake);
    let fd = platform
        .open(b"/a", fcntl::O_RDONLY as u32, 0)
        .expect("open");
    assert!(!isatty(&platform, fd));
}

#[test]
fn bad_fd_is_not_a_tty() {
    let platform = Platform::new(FakeSys::new());
    assert!(!isatty(&platform, 42));
}
