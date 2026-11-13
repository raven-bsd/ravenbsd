// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Socket record tests.

use super::*;
use crate::ErrnoExt;
use raven_libc_abi::socket::{SOCK_DGRAM, SOCK_STREAM};

#[test]
fn new_record_defaults() {
    let rec = SocketRec::new(2, SOCK_STREAM, 0, SocketState::Pending, 5);
    assert_eq!(rec.fd, 5);
    assert!(rec.sock.is_none());
    assert_eq!(rec.so_error, 0);
    assert!(!rec.read_shut());
    assert!(!rec.write_shut());
}

#[test]
fn shutdown_flags_are_visible() {
    let mut rec = SocketRec::new(2, SOCK_STREAM, 0, SocketState::Connected, 3);
    rec.state = SocketState::Shutdown {
        read: true,
        write: false,
    };
    assert!(rec.read_shut());
    assert!(!rec.write_shut());
}

#[test]
fn table_insert_get_remove() {
    let mut table = SocketTable::new();
    assert!(table.is_empty());
    table
        .insert(7, SocketRec::new(2, SOCK_DGRAM, 0, SocketState::Open, 3))
        .expect("insert");
    assert_eq!(table.len(), 1);
    assert!(table.get(7).is_some());
    assert!(table.get(8).is_none());
    assert!(table.get_mut(7).is_some());
    assert!(table.remove(7).is_some());
    assert!(table.is_empty());
}

#[test]
fn table_limit_is_bounded() {
    let mut table = SocketTable::new();
    for index in 0..MAX_SOCKETS {
        table
            .insert(
                index as u32,
                SocketRec::new(2, SOCK_STREAM, 0, SocketState::Open, 0),
            )
            .expect("insert");
    }
    assert_eq!(
        table.insert(
            MAX_SOCKETS as u32,
            SocketRec::new(2, SOCK_STREAM, 0, SocketState::Open, 0)
        ),
        Err(Errno::EMFILE)
    );
}
