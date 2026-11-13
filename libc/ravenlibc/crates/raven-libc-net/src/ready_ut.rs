// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Readiness mapping tests.

use super::*;
use crate::Sock;
use crate::record::{SocketRec, SocketState};
use raven_libc_abi::errno::Errno;
use raven_libc_abi::poll::{POLLERR, POLLHUP, POLLIN, POLLOUT, POLLRDHUP};

fn item() -> WaitItem {
    WaitItem {
        sock: Sock(1),
        ..WaitItem::default()
    }
}

#[test]
fn listener_interest_is_read() {
    let mut rec = SocketRec::new(2, 1, 0, SocketState::Listening, 3);
    rec.sock = Some(Sock(9));
    let interest = interest(&rec).expect("interest");
    assert_eq!(interest.sock, Sock(9));
    assert!(interest.read);
    assert!(interest.write);
}

#[test]
fn pending_socket_has_no_interest() {
    let rec = SocketRec::new(2, 1, 0, SocketState::Pending, 3);
    assert_eq!(interest(&rec), Err(Errno::EBADF));
}

#[test]
fn all_wait_bits_map() {
    let mut ev = item();
    ev.got_read = true;
    ev.got_write = true;
    ev.got_err = true;
    ev.got_hup = true;
    let events = (POLLIN | POLLOUT | POLLRDHUP) as i16;
    let revents = revents(events, &ev);
    assert_ne!(revents & POLLIN as i16, 0);
    assert_ne!(revents & POLLOUT as i16, 0);
    assert_ne!(revents & POLLERR as i16, 0);
    assert_ne!(revents & POLLHUP as i16, 0);
    assert_ne!(revents & POLLRDHUP as i16, 0);
}

#[test]
fn masking_hides_read_and_write_but_not_error() {
    let mut ev = item();
    ev.got_read = true;
    ev.got_write = true;
    ev.got_err = true;
    let only_out = revents(POLLOUT as i16, &ev) as u16;
    assert_eq!(only_out & POLLIN as u16, 0);
    assert_ne!(only_out & POLLOUT as u16, 0);
    assert_ne!(only_out & POLLERR as u16, 0);
}

#[test]
fn hangup_is_always_reported() {
    let mut ev = item();
    ev.got_hup = true;
    let value = revents(0, &ev) as u16;
    assert_ne!(value & POLLHUP as u16, 0);
}
