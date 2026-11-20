// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `poll` tests.

use alloc::vec;

use raven_abi::errno::Errno;
use raven_abi::types::Handle;
use raven_libc_abi::fcntl;
use raven_libc_abi::poll::{POLLHUP, POLLIN, POLLNVAL, POLLOUT};
use raven_libc_abi::structs::Pollfd;

use crate::backend::FakeSys;
use crate::fd::OfdKind;
use crate::platform::Platform;
use crate::poll::{EV_SIGNALED, MAX_POLL_FDS, poll, ppoll};

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
fn ppoll_reports_the_poll_result() {
    let platform = regular_file_platform();
    let mut fds = [Pollfd {
        fd: 0,
        events: POLLIN as i16,
        revents: 0,
    }];
    let timeout = raven_libc_abi::structs::Timespec {
        tv_sec: 0,
        tv_nsec: 1_000_000,
    };
    assert_eq!(ppoll(&platform, &mut fds, Some(&timeout), None), Ok(1));
    assert_eq!(fds[0].revents, POLLNVAL as i16);
}

fn tty_platform() -> (Platform<FakeSys>, i32) {
    let fake = FakeSys::new();
    fake.tty(b"/dev/tty");
    let platform = Platform::new(fake);
    let fd = platform
        .open(b"/dev/tty", fcntl::O_RDWR as u32, 0)
        .expect("open");
    (platform, fd)
}

#[test]
fn two_entries_for_one_descriptor_register_it_once() {
    let (platform, fd) = tty_platform();
    let handle = platform.get(fd).expect("ref").handle();
    platform.backend().ready(handle, 1);
    let mut fds = [
        Pollfd {
            fd,
            events: POLLIN as i16,
            revents: 0,
        },
        Pollfd {
            fd,
            events: POLLOUT as i16,
            revents: 0,
        },
    ];
    assert_eq!(poll(&platform, &mut fds, 100), Ok(1), "not EEXIST");
    assert_eq!(fds[0].revents as i32, POLLIN);
    assert_eq!(fds[1].revents, 0, "the writer entry did not ask for POLLIN");
}

#[test]
fn every_entry_for_a_shared_descriptor_gets_its_own_revents() {
    let (platform, fd) = tty_platform();
    let handle = platform.get(fd).expect("ref").handle();
    platform.backend().ready(handle, 3);
    let mut fds = [
        Pollfd {
            fd,
            events: POLLIN as i16,
            revents: 0,
        },
        Pollfd {
            fd,
            events: POLLOUT as i16,
            revents: 0,
        },
        Pollfd {
            fd,
            events: (POLLIN | POLLOUT) as i16,
            revents: 0,
        },
    ];
    assert_eq!(poll(&platform, &mut fds, 100), Ok(3));
    assert_eq!(fds[0].revents as i32, POLLIN);
    assert_eq!(fds[1].revents as i32, POLLOUT);
    assert_eq!(fds[2].revents as i32, POLLIN | POLLOUT);
}

#[test]
fn a_failed_poll_leaves_the_cached_queue_usable() {
    let (platform, fd) = tty_platform();
    let handle = platform.get(fd).expect("ref").handle();
    for _ in 0..3 {
        platform.backend().ready(handle, 1);
        let mut fds = [
            Pollfd {
                fd,
                events: POLLIN as i16,
                revents: 0,
            },
            Pollfd {
                fd,
                events: POLLOUT as i16,
                revents: 0,
            },
        ];
        assert_eq!(poll(&platform, &mut fds, 100), Ok(1));
    }
}

#[test]
fn an_entry_with_no_events_is_not_ready() {
    let (platform, fd) = tty_platform();
    let mut fds = [
        Pollfd {
            fd,
            events: POLLIN as i16,
            revents: 0,
        },
        Pollfd {
            fd,
            events: 0,
            revents: 0,
        },
    ];
    assert_eq!(poll(&platform, &mut fds, 1), Ok(0));
    assert_eq!(fds[1].revents, 0);
}

#[test]
fn poll_recovers_from_a_foreign_waiting_event_queue() {
    let platform = Platform::new(FakeSys::new());
    let fd = platform
        .install(Handle(7), OfdKind::Socket, 2, None, false)
        .expect("install");
    let ofd = platform.get(fd).expect("fd").index();
    platform.set_ofd_event(ofd, Handle(9)).expect("event");
    platform.backend().ready(Handle(9), EV_SIGNALED);
    platform
        .backend()
        .fail_next(crate::backend::Fault::EvqWait, Errno::EBUSY);
    let mut fds = [Pollfd {
        fd,
        events: POLLIN as i16,
        revents: 0,
    }];
    assert_eq!(poll(&platform, &mut fds, 100), Ok(1));
    assert_eq!(fds[0].revents as i32, POLLIN);
}

#[test]
fn a_delivered_scheme_event_rearms_once() {
    let platform = Platform::new(FakeSys::new());
    let fd = platform
        .install(Handle(7), OfdKind::Socket, 2, None, false)
        .expect("install");
    let ofd = platform.get(fd).expect("fd").index();
    platform.set_ofd_event(ofd, Handle(9)).expect("event");
    platform.backend().ready(Handle(9), EV_SIGNALED);
    let mut fds = [Pollfd {
        fd,
        events: POLLIN as i16,
        revents: 0,
    }];
    assert_eq!(poll(&platform, &mut fds, 100), Ok(1));
    assert_eq!(fds[0].revents as i32, POLLIN);
    // The kernel consumed the event, but the socket may still hold queued level-ready data.
    // The next poll replays the delivered level once.
    fds[0].revents = 0;
    assert_eq!(poll(&platform, &mut fds, 100), Ok(1));
    assert_eq!(fds[0].revents as i32, POLLIN);
    // No busy loop: a third poll with no new event and no rearm times out.
    fds[0].revents = 0;
    assert_eq!(poll(&platform, &mut fds, 1), Ok(0));
    assert_eq!(fds[0].revents, 0);
}
