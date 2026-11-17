// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host tests for the personality-created descriptor modes.

use super::{install_status, is_eventfd_kind};
use raven_libc_personality_linux::seams::FdKind;
use raven_libc_platform::fd::ofd::OfdKind;

#[test]
fn eventfds_are_installed_read_write() {
    assert_eq!(
        install_status(FdKind::EventFd),
        raven_libc_abi::fcntl::O_RDWR as u32
    );
}

#[test]
fn the_other_personality_kinds_keep_the_read_only_status() {
    for kind in [
        FdKind::Epoll,
        FdKind::TimerFd,
        FdKind::SignalFd,
        FdKind::Inotify,
        FdKind::Kqueue,
    ] {
        assert_eq!(install_status(kind), 0);
    }
}

#[test]
fn only_eventfds_route_read_write_through_the_counter_engine() {
    assert!(is_eventfd_kind(OfdKind::EventFd));
    for kind in [
        OfdKind::File,
        OfdKind::Dir,
        OfdKind::Tty,
        OfdKind::Socket,
        OfdKind::Pipe,
        OfdKind::Epoll,
        OfdKind::Timer,
        OfdKind::Unknown,
    ] {
        assert!(!is_eventfd_kind(kind));
    }
}
