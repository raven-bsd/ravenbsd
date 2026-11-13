// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `epoll` engine tests.

use alloc::vec;

use raven_abi::errno::Errno;
use raven_libc_abi::fcntl;
use raven_libc_abi::structs::EpollEvent;

use crate::backend::FakeSys;
use crate::platform::Platform;
use crate::poll::epoll::{
    EPOLL_CTL_ADD, EPOLL_CTL_DEL, EPOLLIN, EPOLLONESHOT, decode_event, encode_event, epoll_create1,
    epoll_ctl, epoll_pwait, epoll_wait,
};

fn file_platform() -> Platform<FakeSys> {
    let fake = FakeSys::new();
    fake.file(b"/a", b"x");
    fake.tty(b"/dev/tty");
    Platform::new(fake)
}

#[test]
fn create_returns_an_epoll_fd() {
    let platform = file_platform();
    let epfd = epoll_create1(&platform, 0).expect("epoll");
    assert!(epfd >= 0);
    assert_eq!(
        platform.get(epfd).expect("ref").kind(),
        crate::fd::ofd::OfdKind::Epoll
    );
}

#[test]
fn create_rejects_unknown_flags() {
    let platform = file_platform();
    assert_eq!(epoll_create1(&platform, 0x1234), Err(Errno::EINVAL));
}

#[test]
fn ctl_add_and_wait_delivers() {
    let platform = file_platform();
    let tty = platform
        .open(b"/dev/tty", fcntl::O_RDWR as u32, 0)
        .expect("tty");
    let handle = platform.get(tty).expect("ref").handle();
    let epfd = epoll_create1(&platform, 0).expect("epoll");
    let event = encode_event(EPOLLIN, 0xdead_beef);
    epoll_ctl(&platform, epfd, EPOLL_CTL_ADD, tty, Some(&event)).expect("ctl");
    platform.backend().ready(handle, 1);
    let mut events = [EpollEvent::ZERO; 4];
    let count = epoll_wait(&platform, epfd, &mut events, 100).expect("wait");
    assert_eq!(count, 1);
    let (flags, data) = decode_event(&events[0]);
    assert_eq!(flags & EPOLLIN, EPOLLIN);
    assert_eq!(data, 0xdead_beef);
}

#[test]
fn ctl_del_removes_the_registration() {
    let platform = file_platform();
    let tty = platform
        .open(b"/dev/tty", fcntl::O_RDWR as u32, 0)
        .expect("tty");
    let handle = platform.get(tty).expect("ref").handle();
    let epfd = epoll_create1(&platform, 0).expect("epoll");
    let event = encode_event(EPOLLIN, 1);
    epoll_ctl(&platform, epfd, EPOLL_CTL_ADD, tty, Some(&event)).expect("add");
    epoll_ctl(&platform, epfd, EPOLL_CTL_DEL, tty, None).expect("del");
    platform.backend().ready(handle, 1);
    let mut events = [EpollEvent::ZERO; 4];
    assert_eq!(epoll_wait(&platform, epfd, &mut events, 1), Ok(0));
}

#[test]
fn oneshot_stops_delivery_after_the_first_event() {
    let platform = file_platform();
    let tty = platform
        .open(b"/dev/tty", fcntl::O_RDWR as u32, 0)
        .expect("tty");
    let handle = platform.get(tty).expect("ref").handle();
    let epfd = epoll_create1(&platform, 0).expect("epoll");
    let event = encode_event(EPOLLIN | EPOLLONESHOT, 2);
    epoll_ctl(&platform, epfd, EPOLL_CTL_ADD, tty, Some(&event)).expect("add");
    platform.backend().ready(handle, 1);
    let mut events = [EpollEvent::ZERO; 4];
    assert_eq!(epoll_wait(&platform, epfd, &mut events, 100), Ok(1));
    platform.backend().ready(handle, 1);
    assert_eq!(epoll_wait(&platform, epfd, &mut events, 100), Ok(0));
}

#[test]
fn stale_cookie_is_dropped() {
    let platform = file_platform();
    let tty = platform
        .open(b"/dev/tty", fcntl::O_RDWR as u32, 0)
        .expect("tty");
    let handle = platform.get(tty).expect("ref").handle();
    let epfd = epoll_create1(&platform, 0).expect("epoll");
    let event = encode_event(EPOLLIN, 3);
    epoll_ctl(&platform, epfd, EPOLL_CTL_ADD, tty, Some(&event)).expect("add");
    platform.close(tty).expect("close");
    platform.backend().ready(handle, 1);
    let mut events = [EpollEvent::ZERO; 4];
    assert_eq!(epoll_wait(&platform, epfd, &mut events, 100), Ok(0));
}

#[test]
fn empty_events_is_einval() {
    let platform = file_platform();
    let epfd = epoll_create1(&platform, 0).expect("epoll");
    assert_eq!(epoll_wait(&platform, epfd, &mut [], 0), Err(Errno::EINVAL));
}

#[test]
fn pwait_is_enosys_until_pg_26() {
    let platform = file_platform();
    let epfd = epoll_create1(&platform, 0).expect("epoll");
    let mut events = vec![EpollEvent::ZERO; 1];
    assert_eq!(
        epoll_pwait(&platform, epfd, &mut events, 0, None),
        Err(Errno::ENOSYS)
    );
}

#[test]
fn event_encoding_round_trips() {
    let event = encode_event(0x1234, 0x5678_9abc_def0);
    let (flags, data) = decode_event(&event);
    assert_eq!(flags, 0x1234);
    assert_eq!(data, 0x5678_9abc_def0);
}
