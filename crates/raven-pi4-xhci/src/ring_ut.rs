// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

use super::*;
use crate::trb::{Trb, TrbType};

#[test]
fn ring_sizes_are_power_of_two_and_bounded() {
    assert_eq!(CommandRing::new(1).unwrap_err(), RingError::TooSmall(1));
    assert_eq!(
        CommandRing::new(128).unwrap_err(),
        RingError::TooManyEntries(128)
    );
    assert_eq!(
        CommandRing::new(6).unwrap_err(),
        RingError::NotPowerOfTwo(6)
    );
    assert!(CommandRing::new(4).is_ok());
    assert!(EventRing::new(8).is_ok());
    assert!(TransferRing::new(2).is_ok());
}

#[test]
fn command_ring_queues_and_completes_in_order() {
    let mut ring = CommandRing::new(4).expect("ring");
    let first = ring.push(Trb::enable_slot()).expect("first");
    let second = ring.push(Trb::address_device(1, 0x1000)).expect("second");
    assert_eq!((first, second), (0, 1));
    assert_eq!(ring.pending(), 2);
    assert_eq!(ring.slot(0).map(Trb::kind), Some(TrbType::EnableSlot));
    assert_eq!(ring.slot(1).map(Trb::slot), Some(1));
    assert_eq!(ring.complete(), Some(0));
    assert_eq!(ring.complete(), Some(1));
    assert_eq!(ring.complete(), None);
    assert_eq!(ring.pending(), 0);
}

#[test]
fn command_ring_wraps_with_a_link_trb_and_toggles_cycle() {
    let mut ring = CommandRing::new(4).expect("ring");
    for _ in 0..3 {
        ring.push(Trb::enable_slot()).expect("push");
    }
    assert_eq!(ring.pending(), 3);
    assert_eq!(
        ring.slot(3).map(Trb::kind),
        Some(TrbType::Link),
        "the reserved slot is a Link TRB"
    );
    assert!(ring.slot(3).expect("link").toggle_cycle());
    assert!(ring.slot(3).expect("link").cycle());
    assert_eq!(ring.enqueue(), 0);
    assert!(!ring.cycle());
}

#[test]
fn command_ring_reports_full() {
    let mut ring = CommandRing::new(4).expect("ring");
    for _ in 0..3 {
        ring.push(Trb::enable_slot()).expect("push");
    }
    assert_eq!(ring.push(Trb::enable_slot()), Err(RingError::Full));
}

#[test]
fn transfer_ring_capacity_includes_the_link_slot() {
    let ring = TransferRing::new(8).expect("ring");
    assert_eq!(ring.capacity(), 8);
    assert_eq!(ring.pending(), 0);
}

#[test]
fn transfer_ring_accepts_a_chained_sequence() {
    let mut ring = TransferRing::new(8).expect("ring");
    ring.push(Trb::setup(0x1122)).expect("setup");
    ring.push(Trb::data(0x2000, 16)).expect("data");
    ring.push(Trb::status(0)).expect("status");
    assert_eq!(ring.slot(0).map(Trb::kind), Some(TrbType::SetupStage));
    assert_eq!(ring.slot(1).map(Trb::length), Some(16));
    assert_eq!(ring.slot(2).map(Trb::kind), Some(TrbType::StatusStage));
}

#[test]
fn event_ring_is_fifo_and_full_is_reported() {
    let mut ring = EventRing::new(4).expect("ring");
    let first = Trb::transfer_event(1, 1, crate::trb::CompletionCode::Success, 8);
    ring.push(first).expect("first");
    ring.push(Trb::port_status_change(2)).expect("second");
    assert_eq!(ring.pending(), 2);
    assert_eq!(ring.pop(), Some(first.with_cycle(true)));
    assert_eq!(
        ring.pop().map(|trb| trb.kind()),
        Some(TrbType::PortStatusChange)
    );
    assert_eq!(ring.pop(), None);
}

#[test]
fn event_ring_wraps_and_flips_the_producer_cycle() {
    let mut ring = EventRing::new(2).expect("ring");
    ring.push(Trb::port_status_change(1)).expect("first");
    ring.push(Trb::port_status_change(2)).expect("second");
    assert_eq!(ring.enqueue(), 0);
    assert!(!ring.cycle());
    ring.pop();
    ring.push(Trb::port_status_change(3)).expect("third");
    assert_eq!(ring.pending(), 2);
    ring.pop();
    ring.pop();
    assert_eq!(ring.pending(), 0);
}

#[test]
fn event_ring_reports_full_without_overwriting() {
    let mut ring = EventRing::new(2).expect("ring");
    ring.push(Trb::port_status_change(1)).expect("first");
    ring.push(Trb::port_status_change(2)).expect("second");
    assert_eq!(ring.push(Trb::port_status_change(3)), Err(RingError::Full));
    assert_eq!(ring.pop().map(|trb| trb.slot()), Some(0));
}

#[test]
fn complete_wraps_the_consumer_index() {
    let mut ring = CommandRing::new(4).expect("ring");
    for _ in 0..3 {
        ring.push(Trb::enable_slot()).expect("push");
    }
    for _ in 0..3 {
        ring.complete().expect("complete");
    }
    let index = ring.push(Trb::disable_slot()).expect("wrap");
    assert_eq!(index, 0);
    assert!(!ring.slot(0).expect("slot").cycle());
}
