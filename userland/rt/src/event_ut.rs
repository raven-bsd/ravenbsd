// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host-side checks for the S26 wrapper constants and record layout.

use super::*;

#[test]
fn event_record_is_sixteen_bytes_with_zero_padding() {
    assert_eq!(core::mem::size_of::<EvqEvent>(), 16);
    let event = EvqEvent {
        cookie: 7,
        events: READABLE | SIGNALED,
        pad: 0,
    };
    assert_eq!(event.cookie, 7);
    assert_eq!(event.events, 9);
}

#[test]
fn readiness_bits_are_distinct() {
    assert_eq!(READABLE, 1);
    assert_eq!(WRITABLE, 2);
    assert_eq!(HUP, 4);
    assert_eq!(SIGNALED, 8);
    assert_eq!(EDGE, 0x8000_0000);
    assert_eq!(READABLE | WRITABLE | HUP | SIGNALED, 0x0f);
}

#[test]
fn event_queue_handle_roundtrips() {
    let queue = EventQueue {
        handle: Handle::new(3, 2),
    };
    assert_eq!(queue.handle(), Handle::new(3, 2));
}
