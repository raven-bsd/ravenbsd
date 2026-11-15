// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host tests for the RX/TX frame ownership and refill model.

use super::*;
use crate::ring::{
    RX_STATUS_BUFLEN_SHIFT, RX_STATUS_CRC_ERR, RX_STATUS_EOP, RX_STATUS_SOP, TX_STATUS_OWN,
};

fn rx_frame(length: u32) -> RxStatus {
    RxStatus::from_raw((length << RX_STATUS_BUFLEN_SHIFT) | RX_STATUS_SOP | RX_STATUS_EOP)
}

#[test]
fn a_zero_or_oversized_slot_count_is_rejected() {
    assert_eq!(RxPath::new(0), Err(FrameError::Ring(RingError::ZeroCount)));
    assert_eq!(
        RxPath::new(DESC_COUNT + 1),
        Err(FrameError::Ring(RingError::TooManyEntries))
    );
    assert_eq!(TxPath::new(0), Err(FrameError::Ring(RingError::ZeroCount)));
    assert_eq!(
        TxPath::new(DESC_COUNT + 1),
        Err(FrameError::Ring(RingError::TooManyEntries))
    );
}

#[test]
fn posting_hands_the_next_slot_to_the_device() {
    let mut path = RxPath::new(256).expect("path");
    let post = path.post_next(0x1000).expect("post");
    assert_eq!(post.index, 0);
    assert_eq!(post.descriptor.address, 0x1000);
    assert_eq!(path.slot_owner(0), Some(SlotOwner::Device));
    assert_eq!(
        path.descriptor(0).map(|descriptor| descriptor.address),
        Some(0x1000)
    );
    assert_eq!(path.posted(), Ok(1));
    assert_eq!(path.indices().producer(), 1);
}

#[test]
fn a_completed_rx_frame_is_delivered_and_the_slot_is_freed() {
    let mut path = RxPath::new(256).expect("path");
    path.post_next(0x1000).expect("post");
    let completion = path
        .complete_next(rx_frame(64))
        .expect("complete")
        .expect("entry");
    assert_eq!(completion.index, 0);
    assert_eq!(completion.frame.expect("frame").length, 62);
    assert_eq!(path.slot_owner(0), Some(SlotOwner::Free));
    assert_eq!(path.posted(), Ok(0));
    assert_eq!(path.indices().consumer(), 1);
    assert_eq!(path.free_slots(), Ok(256));
}

#[test]
fn a_still_owned_rx_descriptor_is_not_advanced() {
    let mut path = RxPath::new(256).expect("path");
    path.post_next(0x1000).expect("post");
    let status = RxStatus::from_raw(rx_frame(64).raw() | crate::ring::RX_STATUS_OWN);
    assert_eq!(path.complete_next(status), Err(FrameError::StillOwned(0)));
    assert_eq!(path.slot_owner(0), Some(SlotOwner::Device));
    assert_eq!(path.posted(), Ok(1));
}

#[test]
fn a_malformed_rx_status_is_reported_but_the_slot_is_freed() {
    let mut path = RxPath::new(256).expect("path");
    path.post_next(0x1000).expect("post");
    let status = RxStatus::from_raw(rx_frame(64).raw() | RX_STATUS_CRC_ERR);
    let completion = path
        .complete_next(status)
        .expect("complete")
        .expect("entry");
    assert!(matches!(completion.frame, Err(RxError::Device(_))));
    assert_eq!(path.slot_owner(0), Some(SlotOwner::Free));
    assert_eq!(path.posted(), Ok(0));
}

#[test]
fn a_short_rx_status_is_reported_but_the_slot_is_freed() {
    let mut path = RxPath::new(256).expect("path");
    path.post_next(0x1000).expect("post");
    let completion = path
        .complete_next(rx_frame(1))
        .expect("complete")
        .expect("entry");
    assert_eq!(completion.frame, Err(RxError::TooShort(1)));
    assert_eq!(path.slot_owner(0), Some(SlotOwner::Free));
}

#[test]
fn an_empty_rx_ring_has_no_completion() {
    let mut path = RxPath::new(4).expect("path");
    assert_eq!(path.complete_next(rx_frame(64)), Ok(None));
}

#[test]
fn rx_refill_wraps_over_the_ring() {
    let mut path = RxPath::new(2).expect("path");
    assert_eq!(path.post_next(0x1000).expect("post").index, 0);
    assert_eq!(path.post_next(0x2000).expect("post").index, 1);
    path.complete_next(rx_frame(64)).expect("complete");
    path.complete_next(rx_frame(64)).expect("complete");
    assert_eq!(path.posted(), Ok(0));
    assert_eq!(path.indices().producer(), 2);
    assert_eq!(path.indices().consumer(), 2);
    assert_eq!(path.refill_next(0x3000).expect("refill").index, 0);
    assert_eq!(path.refill_next(0x4000).expect("refill").index, 1);
    assert_eq!(
        path.descriptor(0).map(|descriptor| descriptor.address),
        Some(0x3000)
    );
    assert_eq!(
        path.descriptor(1).map(|descriptor| descriptor.address),
        Some(0x4000)
    );
    assert_eq!(path.posted(), Ok(2));
}

#[test]
fn a_full_rx_ring_rejects_another_post() {
    let mut path = RxPath::new(1).expect("path");
    path.post_next(0x1000).expect("post");
    assert_eq!(path.post_next(0x2000), Err(FrameError::NoFreeSlot));
}

#[test]
fn a_zero_or_unaligned_address_is_rejected() {
    let mut path = RxPath::new(4).expect("path");
    assert_eq!(path.post_next(0), Err(FrameError::Address(0)));
    assert_eq!(path.post_next(0x1008), Err(FrameError::Address(0x1008)));
}

#[test]
fn a_completed_tx_frame_is_reclaimed_and_the_slot_is_freed() {
    let mut path = TxPath::new(256).expect("path");
    let post = path.queue_next(0x8000, 64).expect("queue");
    assert_eq!(post.index, 0);
    assert_eq!(post.descriptor.status.length(), 64);
    assert_eq!(path.slot_owner(0), Some(SlotOwner::Device));
    let completion = path
        .reclaim_next(post.descriptor.status)
        .expect("reclaim")
        .expect("entry");
    assert_eq!(completion.index, 0);
    assert_eq!(completion.status.length(), 64);
    assert_eq!(path.slot_owner(0), Some(SlotOwner::Free));
    assert_eq!(path.posted(), Ok(0));
}

#[test]
fn a_still_owned_tx_descriptor_is_not_advanced() {
    let mut path = TxPath::new(4).expect("path");
    let post = path.queue_next(0x8000, 64).expect("queue");
    let owned = TxStatus::from_raw(post.descriptor.status.raw() | TX_STATUS_OWN);
    assert_eq!(path.reclaim_next(owned), Err(FrameError::StillOwned(0)));
    assert_eq!(path.slot_owner(0), Some(SlotOwner::Device));
    assert_eq!(path.posted(), Ok(1));
}

#[test]
fn tx_queue_and_reclaim_wrap_over_the_ring() {
    let mut path = TxPath::new(2).expect("path");
    let first = path.queue_next(0x8000, 64).expect("queue");
    let second = path.queue_next(0x8800, 128).expect("queue");
    assert_eq!(first.index, 0);
    assert_eq!(second.index, 1);
    path.reclaim_next(first.descriptor.status).expect("reclaim");
    path.reclaim_next(second.descriptor.status)
        .expect("reclaim");
    assert_eq!(path.indices().producer(), 2);
    assert_eq!(path.indices().consumer(), 2);
    assert_eq!(path.queue_next(0x9000, 64).expect("queue").index, 0);
    assert_eq!(path.queue_next(0x9800, 64).expect("queue").index, 1);
}

#[test]
fn a_zero_or_oversized_tx_length_is_rejected() {
    let mut path = TxPath::new(4).expect("path");
    assert_eq!(path.queue_next(0x8000, 0), Err(FrameError::Length(0)));
    assert_eq!(
        path.queue_next(0x8000, BUFFER_LEN + 1),
        Err(FrameError::Length(BUFFER_LEN + 1))
    );
}

#[test]
fn an_empty_tx_ring_has_no_completion() {
    let mut path = TxPath::new(4).expect("path");
    assert_eq!(
        path.reclaim_next(TxStatus::single(64).expect("status")),
        Ok(None)
    );
}

#[test]
fn frame_path_creates_both_directions() {
    let path = FramePath::new(8).expect("path");
    assert_eq!(path.rx.count(), 8);
    assert_eq!(path.tx.count(), 8);
    assert_eq!(path.rx.free_slots(), Ok(8));
    assert_eq!(path.tx.free_slots(), Ok(8));
}

#[test]
fn an_out_of_range_slot_has_no_owner_or_descriptor() {
    let path = RxPath::new(2).expect("path");
    assert_eq!(path.slot_owner(2), None);
    assert_eq!(path.descriptor(2), None);
    let path = TxPath::new(2).expect("path");
    assert_eq!(path.slot_owner(2), None);
    assert_eq!(path.descriptor(2), None);
}
