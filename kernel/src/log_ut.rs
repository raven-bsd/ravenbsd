// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Unit tests for the kernel log ring and the panic-path try-lock.

use core::fmt::Write as _;

use super::*;

fn body(fill: u8, len: usize) -> [u8; RECORD_MAX_BODY + 1] {
    let mut bytes = [fill; RECORD_MAX_BODY + 1];
    for slot in bytes.iter_mut().skip(len) {
        *slot = 0;
    }
    bytes
}

#[test]
fn ring_wrap_drops_oldest_and_reports_lost() {
    let mut ring = Ring::<128>::new();
    let text = body(b'a', 40);
    assert!(ring.write_at(1, 2, 0, &text[..40]));
    assert!(ring.write_at(2, 2, 0, &text[..40]));
    assert!(ring.write_at(3, 2, 0, &text[..40]));

    assert_eq!(ring.first_cursor(), 56);
    assert_eq!(ring.last_cursor(), 168);
    assert_eq!(ring.dropped(), 1);

    let plan = ring.plan_read(0, 4096);
    assert!(plan.lost);
    assert_eq!(plan.start, 56);
    assert_eq!(plan.bytes, 112);
    assert_eq!(plan.next, 168);
    assert!(!plan.more);
}

#[test]
fn oversized_record_is_not_written_and_is_dropped() {
    let mut ring = Ring::<128>::new();
    let text = body(b'x', RECORD_MAX_BODY + 1);
    assert!(!ring.write_at(1, 2, 0, &text));
    assert_eq!(ring.first_cursor(), 0);
    assert_eq!(ring.last_cursor(), 0);
    assert_eq!(ring.dropped(), 1);
}

#[test]
fn more_is_set_when_the_next_record_does_not_fit() {
    let mut ring = Ring::<128>::new();
    let text = body(b'm', 8);
    assert!(ring.write_at(1, 2, 0, &text[..8]));

    let plan = ring.plan_read(0, 16);
    assert_eq!(plan.start, 0);
    assert_eq!(plan.next, 0);
    assert_eq!(plan.bytes, 0);
    assert!(!plan.lost);
    assert!(plan.more);
}

#[test]
fn record_header_carries_cursor_level_and_panic_flag() {
    let mut ring = Ring::<128>::new();
    assert!(ring.write_at(123, 1, RECORD_FLAG_PANIC, b"xy"));
    let size = ring.record_size(0).expect("record");
    let mut record = [0u8; 64];
    ring.copy_record(0, record.get_mut(..size).expect("record slice"))
        .expect("copy");
    assert_eq!(record.get(..8), Some(123u64.to_le_bytes().as_slice()));
    assert_eq!(record.get(8), Some(&1));
    assert_eq!(record.get(9), Some(&RECORD_FLAG_PANIC));
    assert_eq!(record.get(10..12), Some(2u16.to_le_bytes().as_slice()));
    assert_eq!(record.get(16..18), Some(b"xy".as_slice()));
}

#[test]
fn record_body_survives_a_physical_wrap() {
    let mut ring = Ring::<64>::new();
    let first = body(b'1', 40);
    let second = body(b'2', 40);
    assert!(ring.write_at(1, 2, 0, &first[..40]));
    assert!(ring.write_at(2, 2, 0, &second[..40]));

    assert_eq!(ring.first_cursor(), 56);
    let size = ring.record_size(56).expect("wrapped record");
    let mut record = [0u8; 64];
    ring.copy_record(56, record.get_mut(..size).expect("record slice"))
        .expect("copy");
    assert_eq!(record.get(16..size), Some(&second[..40]));
}

#[test]
fn cursor_past_the_end_is_clamped_without_lost() {
    let mut ring = Ring::<128>::new();
    let text = body(b'e', 8);
    assert!(ring.write_at(1, 2, 0, &text[..8]));
    let plan = ring.plan_read(999, 4096);
    assert!(!plan.lost);
    assert_eq!(plan.start, 24);
    assert_eq!(plan.next, 24);
    assert_eq!(plan.bytes, 0);
}

#[test]
fn panic_record_try_lock_does_not_wait_for_a_held_ring() {
    let mut line = LineBuffer::new();
    let _ = line.write_str("panic while the ring is held");
    let guard = RING.lock();
    assert!(!record_panic(&line));
    drop(guard);
}
