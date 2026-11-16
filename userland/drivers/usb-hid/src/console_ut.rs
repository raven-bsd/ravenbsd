// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Unit tests for the console input queue.

use super::*;

#[test]
fn bytes_are_fifo() {
    let mut queue = ConsoleInput::new();
    assert!(queue.is_empty());
    assert!(queue.push(b'a'));
    assert!(queue.push(b'b'));
    assert_eq!(queue.len(), 2);
    let mut out = [0u8; 4];
    assert_eq!(queue.read(&mut out), 2);
    assert_eq!(&out[..2], b"ab");
    assert!(queue.is_empty());
}

#[test]
fn the_queue_wraps() {
    let mut queue = ConsoleInput::new();
    for byte in 0..10u8 {
        assert!(queue.push(byte));
    }
    let mut out = [0u8; 4];
    assert_eq!(queue.read(&mut out), 4);
    assert_eq!(out, [0, 1, 2, 3]);
    for byte in 10..14u8 {
        assert!(queue.push(byte));
    }
    let mut rest = [0u8; 16];
    assert_eq!(queue.read(&mut rest), 10);
    assert_eq!(&rest[..10], &[4, 5, 6, 7, 8, 9, 10, 11, 12, 13]);
}

#[test]
fn a_full_queue_drops_new_bytes() {
    let mut queue = ConsoleInput::new();
    for index in 0..CONSOLE_QUEUE {
        assert!(queue.push(index as u8));
    }
    assert_eq!(queue.len(), CONSOLE_QUEUE);
    assert!(!queue.push(0xff));
    let mut out = [0u8; CONSOLE_QUEUE];
    assert_eq!(queue.read(&mut out), CONSOLE_QUEUE);
    assert_eq!(out[0], 0);
    assert_eq!(out[CONSOLE_QUEUE - 1], (CONSOLE_QUEUE - 1) as u8);
    assert!(queue.is_empty());
}

#[test]
fn read_is_bounded_and_clear_empties() {
    let mut queue = ConsoleInput::new();
    assert!(queue.push(b'x'));
    assert!(queue.push(b'y'));
    let mut out = [0u8; 1];
    assert_eq!(queue.read(&mut out), 1);
    assert_eq!(out[0], b'x');
    assert_eq!(queue.len(), 1);
    queue.clear();
    assert!(queue.is_empty());
    assert_eq!(queue.pop(), None);
}
