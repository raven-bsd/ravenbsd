// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host tests for the personality-created descriptor modes.

use super::{bounded_hi, install_status, is_eventfd_kind};
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

#[test]
fn close_range_hi_is_clamped_to_the_hard_descriptor_limit() {
    // No descriptor at or above the hard RLIMIT exists: INT_MAX must not be iterated.
    assert_eq!(bounded_hi(i32::MAX, 32768), 32767);
    assert_eq!(bounded_hi(i32::MAX, 1024), 1023);
}

#[test]
fn close_range_hi_keeps_a_smaller_request() {
    assert_eq!(bounded_hi(10, 32768), 10);
    assert_eq!(bounded_hi(0, 32768), 0);
}

#[test]
fn close_range_hi_handles_a_zero_hard_limit() {
    assert_eq!(bounded_hi(5, 0), 0);
}
