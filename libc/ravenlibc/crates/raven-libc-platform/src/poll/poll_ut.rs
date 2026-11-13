// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `poll` tests.

use alloc::vec;

use raven_abi::errno::Errno;
use raven_libc_abi::fcntl;
use raven_libc_abi::poll::{POLLHUP, POLLIN, POLLNVAL, POLLOUT};
use raven_libc_abi::structs::Pollfd;

use crate::backend::FakeSys;
use crate::platform::Platform;
use crate::poll::{MAX_POLL_FDS, poll, ppoll};

fn regular_file_platform() -> Platform<FakeSys> {
    let fake = FakeSys::new();
    fake.file(b"/a", b"x");
    Platform::new(fake)
}

#[test]
fn regular_file_is_immediately_readable() {
    let platform = regular_file_platform();
    let fd = platform
        .open(b"/a", fcntl::O_RDONLY as u32, 0)
        .expect("open");
    let mut fds = [Pollfd {
        fd,
        events: POLLIN as i16,
        revents: 0,
    }];
    assert_eq!(poll(&platform, &mut fds, 1000), Ok(1));
    assert_eq!(fds[0].revents as i32, POLLIN);
}

#[test]
fn regular_file_is_immediately_writable() {
    let platform = regular_file_platform();
    let fd = platform.open(b"/a", fcntl::O_RDWR as u32, 0).expect("open");
    let mut fds = [Pollfd {
        fd,
        events: POLLOUT as i16,
        revents: 0,
    }];
    assert_eq!(poll(&platform, &mut fds, 1000), Ok(1));
    assert_eq!(fds[0].revents as i32, POLLOUT);
}

#[test]
fn invalid_fd_is_pollnval() {
    let platform = regular_file_platform();
    let mut fds = [Pollfd {
        fd: 99,
        events: POLLIN as i16,
        revents: 0,
    }];
    assert_eq!(poll(&platform, &mut fds, 0), Ok(1));
    assert_eq!(fds[0].revents as i32, POLLNVAL);
}

#[test]
fn negative_fd_is_ignored() {
    let platform = regular_file_platform();
    let mut fds = [Pollfd {
        fd: -1,
        events: POLLIN as i16,
        revents: 0x7f,
    }];
    assert_eq!(poll(&platform, &mut fds, 0), Ok(0));
    assert_eq!(fds[0].revents, 0);
}

#[test]
fn tty_hup_is_reported() {
    let fake = FakeSys::new();
    fake.tty(b"/dev/tty");
    let platform = Platform::new(fake);
    let fd = platform
        .open(b"/dev/tty", fcntl::O_RDWR as u32, 0)
        .expect("open");
    let handle = platform.get(fd).expect("ref").handle();
    platform.backend().ready(handle, 4);
    let mut fds = [Pollfd {
        fd,
        events: POLLIN as i16,
        revents: 0,
    }];
    assert_eq!(poll(&platform, &mut fds, 100), Ok(1));
    assert_eq!(fds[0].revents as i32, POLLHUP);
}

#[test]
fn tty_timeout_returns_zero() {
    let fake = FakeSys::new();
    fake.tty(b"/dev/tty");
    let platform = Platform::new(fake);
    let fd = platform
        .open(b"/dev/tty", fcntl::O_RDWR as u32, 0)
        .expect("open");
    let mut fds = [Pollfd {
        fd,
        events: POLLIN as i16,
        revents: 0,
    }];
    assert_eq!(poll(&platform, &mut fds, 1), Ok(0));
    assert_eq!(fds[0].revents, 0);
}

#[test]
fn too_many_fds_is_einval() {
    let platform = regular_file_platform();
    let mut fds = vec![
        Pollfd {
            fd: -1,
            events: 0,
            revents: 0
        };
        MAX_POLL_FDS + 1
    ];
    assert_eq!(poll(&platform, &mut fds, 0), Err(Errno::EINVAL));
}

#[test]
fn ppoll_is_enosys_until_pg_26() {
    let platform = regular_file_platform();
    let mut fds = [Pollfd {
        fd: 0,
        events: POLLIN as i16,
        revents: 0,
    }];
    assert_eq!(ppoll(&platform, &mut fds, None, None), Err(Errno::ENOSYS));
}
