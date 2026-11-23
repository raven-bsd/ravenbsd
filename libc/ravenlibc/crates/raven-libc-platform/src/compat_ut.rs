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
fn isatty_probes_an_unknown_kind_pty() {
    // The v1 spawn map marks the inherited pty Unknown; `isatty` must ask the terminal instead
    // of rejecting it on the kind, or uutils `tty` never reaches `ttyname`.
    let fake = FakeSys::new();
    fake.tty(b"/dev/tty");
    let platform = Platform::new(fake);
    let _first = platform
        .open(b"/dev/tty", fcntl::O_RDWR as u32, 0)
        .expect("first");
    let second = platform
        .open(b"/dev/tty", fcntl::O_RDWR as u32, 0)
        .expect("second");
    let handle = platform.get(second).expect("fd").handle();
    let fd = platform
        .install(
            handle,
            crate::fd::ofd::OfdKind::Unknown,
            fcntl::O_RDWR as u32,
            None,
            false,
        )
        .expect("install");
    assert!(crate::tty::isatty(&platform, fd));
    assert!(platform.tcgetattr(fd).is_ok());
}

#[test]
fn pty_pair_names_a_master_and_slave_from_the_service() {
    let fake = FakeSys::new();
    fake.tty(b"/dev/tty");
    let platform = Platform::new(fake);
    let first = platform
        .open(b"/dev/tty", fcntl::O_RDWR as u32, 0)
        .expect("first");
    let second = platform
        .open(b"/dev/tty", fcntl::O_RDWR as u32, 0)
        .expect("second");
    let master_handle = platform.get(first).expect("fd").handle();
    let slave_handle = platform.get(second).expect("fd").handle();
    let master = platform
        .install(
            master_handle,
            crate::fd::ofd::OfdKind::Unknown,
            fcntl::O_RDWR as u32,
            None,
            false,
        )
        .expect("master");
    let slave = platform
        .install(
            slave_handle,
            crate::fd::ofd::OfdKind::Unknown,
            fcntl::O_RDWR as u32,
            None,
            false,
        )
        .expect("slave");
    // Handle 5 -> ino 105 -> pair 52 master; handle 6 -> ino 106 -> pair 52 slave.
    assert_eq!(crate::compat::pty_pair(&platform, master), Some((52, true)));
    assert_eq!(crate::compat::pty_pair(&platform, slave), Some((52, false)));
}

#[test]
fn ttyname_names_a_live_pty_pair_from_the_service() {
    // An inherited terminal has no `ofd.path`; the pty service knows the live slave pair from
    // its endpoint identity (slave = 2N+2). FakeSys reports `ino = handle + 100`, so the second
    // open (handle 6, ino 106) is pair 52.
    let fake = FakeSys::new();
    fake.tty(b"/dev/tty");
    let platform = Platform::new(fake);
    let _first = platform
        .open(b"/dev/tty", fcntl::O_RDWR as u32, 0)
        .expect("first");
    let second = platform
        .open(b"/dev/tty", fcntl::O_RDWR as u32, 0)
        .expect("second");
    let handle = platform.get(second).expect("fd").handle();
    let fd = platform
        .install(
            handle,
            crate::fd::ofd::OfdKind::Unknown,
            fcntl::O_RDWR as u32,
            None,
            false,
        )
        .expect("install");
    assert_eq!(
        crate::compat::ttyname(&platform, fd),
        Ok(b"/dev/pts/52".to_vec())
    );
}

#[test]
fn ttyname_does_not_name_a_legacy_console_as_a_pty() {
    // The console answers GET_MODE only; it is not a pty endpoint and has no /dev/pts name.
    let fake = FakeSys::new();
    fake.tty_legacy(b"/dev/console");
    let platform = Platform::new(fake);
    let _first = platform
        .open(b"/dev/console", fcntl::O_RDWR as u32, 0)
        .expect("first");
    let second = platform
        .open(b"/dev/console", fcntl::O_RDWR as u32, 0)
        .expect("second");
    let handle = platform.get(second).expect("fd").handle();
    let fd = platform
        .install(
            handle,
            crate::fd::ofd::OfdKind::Unknown,
            fcntl::O_RDWR as u32,
            None,
            false,
        )
        .expect("install");
    assert_eq!(
        crate::compat::ttyname(&platform, fd),
        Err(crate::Errno::ENOTTY)
    );
}
