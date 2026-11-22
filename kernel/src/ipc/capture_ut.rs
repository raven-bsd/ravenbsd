// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host tests for the capture-slot page allocator.

use super::*;

#[test]
fn two_captures_share_one_slot_without_overlapping_pages() {
    let mut slots = CaptureSlots::new();
    let request = slots.acquire(1).expect("request range");
    let reply = slots.acquire(1).expect("reply range");
    assert_eq!(request.slot, reply.slot);
    assert_ne!(request.page, reply.page);
    let stats = slots.stats();
    assert_eq!(stats.captures, 2);
    assert_eq!(stats.pages, 2);
}

#[test]
fn a_slot_exhausts_by_pages_then_the_next_slot_is_used() {
    let mut slots = CaptureSlots::new();
    let first = slots.acquire(SLOT_PAGES as u64).expect("whole first slot");
    let second = slots.acquire(SLOT_PAGES as u64).expect("whole second slot");
    assert_ne!(first.slot, second.slot);
    assert_eq!(slots.stats().pages, SLOT_PAGES * 2);
}

#[test]
fn oversized_capture_is_rejected_without_taking_pages() {
    let mut slots = CaptureSlots::new();
    assert!(slots.acquire(SLOT_PAGES as u64 + 1).is_none());
    assert_eq!(slots.stats().pages, 0);
}

#[test]
fn release_returns_the_range_to_the_pool() {
    let mut slots = CaptureSlots::new();
    let range = slots.acquire(2).expect("range");
    slots.release(range);
    assert_eq!(slots.stats().captures, 0);
    assert_eq!(slots.acquire(2), Some(range));
}

#[test]
fn double_release_is_idempotent() {
    let mut slots = CaptureSlots::new();
    let range = slots.acquire(1).expect("range");
    slots.release(range);
    slots.release(range);
    assert_eq!(slots.stats().captures, 0);
    assert_eq!(slots.acquire(1), Some(range));
}

#[test]
fn peak_tracks_captures_and_pages() {
    let mut slots = CaptureSlots::new();
    let first = slots.acquire(2).expect("first");
    let second = slots.acquire(3).expect("second");
    slots.release(first);
    let stats = slots.stats();
    assert_eq!(stats.captures, 1);
    assert_eq!(stats.capture_peak, 2);
    assert_eq!(stats.pages, 3);
    assert_eq!(stats.page_peak, 5);
    slots.release(second);
    assert_eq!(slots.stats().captures, 0);
}

#[test]
fn unrelated_mapping_keeps_root_and_copy_back_per_page() {
    let captured_root = 0x1000;
    let captured_frame = 0x7000;
    // Another thread's MAP/UNMAP changes the mapping generation but not the page-table root.
    assert!(address_space_matches(captured_root, Some(captured_root)));
    // The captured page still translates to the same writable frame, so copy-back proceeds.
    assert!(page_copy_allowed(
        captured_frame,
        Some(captured_frame),
        true
    ));
}

#[test]
fn replaced_capture_mapping_is_skipped() {
    let captured_frame = 0x7000;
    let replacement = 0x8000;
    assert!(!page_copy_allowed(captured_frame, Some(replacement), true));
    assert!(!page_copy_allowed(captured_frame, None, true));
    assert!(!page_copy_allowed(
        captured_frame,
        Some(captured_frame),
        false
    ));
}

#[test]
fn process_exit_and_exec_drop_copy_back() {
    let captured_root = 0x1000;
    assert!(!address_space_matches(captured_root, None));
    assert!(!address_space_matches(captured_root, Some(0x2000)));
}
