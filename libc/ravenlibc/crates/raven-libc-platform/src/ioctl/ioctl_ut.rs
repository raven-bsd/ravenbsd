// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `ioctl` dispatch tests.

use raven_abi::errno::Errno;
use raven_libc_abi::fcntl;

use crate::backend::FakeSys;
use crate::ioctl::{ioctl, ioctl_tcgets, ioctl_tcsets};
use crate::platform::Platform;
use crate::xlat;

#[test]
fn fionbio_sets_and_clears_nonblock() {
    let fake = FakeSys::new();
    fake.file(b"/a", b"x");
    let platform = Platform::new(fake);
    let fd = platform
        .open(b"/a", fcntl::O_RDONLY as u32, 0)
        .expect("open");
    ioctl(&platform, fd, xlat::FIONBIO, 1).expect("set");
    let status = platform.get(fd).expect("ref").status();
    assert_ne!(status & fcntl::O_NONBLOCK as u32, 0);
    ioctl(&platform, fd, xlat::FIONBIO, 0).expect("clear");
    let status = platform.get(fd).expect("ref").status();
    assert_eq!(status & fcntl::O_NONBLOCK as u32, 0);
}

#[test]
fn fioclex_and_fionclex() {
    let fake = FakeSys::new();
    fake.file(b"/a", b"x");
    let platform = Platform::new(fake);
    let fd = platform
        .open(b"/a", fcntl::O_RDONLY as u32, 0)
        .expect("open");
    ioctl(&platform, fd, xlat::FIOCLEX, 0).expect("set");
    assert!(platform.cloexec(fd).expect("cloexec"));
    ioctl(&platform, fd, xlat::FIONCLEX, 0).expect("clear");
    assert!(!platform.cloexec(fd).expect("cloexec"));
}

#[test]
fn unknown_request_is_enotty() {
    let platform = Platform::new(FakeSys::new());
    assert_eq!(ioctl(&platform, 0, 0xdead, 0), Err(Errno::ENOTTY));
}

#[test]
fn tcgets_needs_a_tty() {
    let fake = FakeSys::new();
    fake.file(b"/a", b"x");
    let platform = Platform::new(fake);
    let fd = platform
        .open(b"/a", fcntl::O_RDONLY as u32, 0)
        .expect("open");
    assert_eq!(ioctl_tcgets(&platform, fd), Err(Errno::ENOTTY));
}

#[test]
fn tcsets_needs_a_tty() {
    let fake = FakeSys::new();
    fake.file(b"/a", b"x");
    let platform = Platform::new(fake);
    let fd = platform
        .open(b"/a", fcntl::O_RDONLY as u32, 0)
        .expect("open");
    let termios = crate::ioctl::tty::default_termios();
    assert_eq!(ioctl_tcsets(&platform, fd, &termios), Err(Errno::ENOTTY));
}

#[test]
fn net_ioctl_classification() {
    assert!(crate::ioctl::net::classify(0x8913).is_some());
    assert!(crate::ioctl::net::classify(0x1234).is_none());
    assert_eq!(crate::ioctl::net::net_ioctl(0x8913), Errno::ENOSYS);
    assert_eq!(crate::ioctl::net::net_ioctl(0x1234), Errno::ENOTTY);
}
