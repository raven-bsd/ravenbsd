// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Assertions for the command, transfer and event rings.

use super::*;
use crate::trb::Trb;

#[test]
fn ring_sizes_are_validated() {
    assert_eq!(CommandRing::new(1).err(), Some(RingError::TooSmall(1)));
    assert_eq!(CommandRing::new(3).err(), Some(RingError::NotPowerOfTwo(3)));
    assert_eq!(
        CommandRing::new(MAX_RING_ENTRIES + 1).err(),
        Some(RingError::TooManyEntries(MAX_RING_ENTRIES + 1))
    );
    assert!(EventRing::new(2).is_ok());
    assert!(TransferRing::new(8).is_ok());
}

#[test]
fn a_command_ring_wraps_through_its_link() {
    let mut ring = CommandRing::new(4).expect("ring");
    assert_eq!(ring.push(Trb::enable_slot()), Ok(0));
    assert_eq!(
        ring.push(Trb::address_device(1, 0x1000).expect("address")),
        Ok(1)
    );
    assert_eq!(ring.push(Trb::normal(0x2000, 0x40, true)), Ok(2));
    assert_eq!(ring.pending(), 3);
    assert_eq!(ring.enqueue(), 0);
    assert!(!ring.cycle(), "the third command wraps the producer cycle");
    assert_eq!(
        ring.slot(3).map(|trb| trb.kind()),
        Some(crate::trb::TrbType::Link)
    );
    assert_eq!(ring.push(Trb::enable_slot()), Err(RingError::Full));

    assert_eq!(ring.complete(), Some(0));
    assert_eq!(ring.complete(), Some(1));
    assert_eq!(ring.complete(), Some(2));
    assert_eq!(ring.complete(), None);

    assert_eq!(ring.push(Trb::enable_slot()), Ok(0));
    assert_eq!(
        ring.slot(0).map(|trb| trb.cycle()),
        Some(false),
        "the new cycle is stamped on the reused slot"
    );
}

#[test]
fn a_transfer_ring_tracks_completions_in_order() {
    let mut ring = TransferRing::new(4).expect("ring");
    assert_eq!(ring.push(Trb::normal(0x1000, 8, true)), Ok(0));
    assert_eq!(ring.push(Trb::normal(0x2000, 8, true)), Ok(1));
    assert_eq!(ring.pending(), 2);
    assert_eq!(ring.complete(), Some(0));
    assert_eq!(ring.complete(), Some(1));
    assert_eq!(ring.complete(), None);
    assert_eq!(ring.pending(), 0);
}

#[test]
fn the_event_ring_is_fifo_and_cycle_checked() {
    let mut ring = EventRing::new(4).expect("ring");
    assert_eq!(
        ring.push_event(Trb::transfer_event(1, 2, 1, 0x1000).expect("event")),
        Ok(0)
    );
    assert_eq!(
        ring.push_event(Trb::transfer_event(1, 3, 1, 0x2000).expect("event")),
        Ok(1)
    );
    assert_eq!(ring.pending(), 2);

    let (index, first) = ring.pop().expect("first");
    assert_eq!(index, 0);
    assert_eq!(first.kind(), crate::trb::TrbType::TransferEvent);
    assert!(first.cycle());
    let (index, second) = ring.pop().expect("second");
    assert_eq!(index, 1);
    assert_eq!(second.slot(), 1);
    assert_eq!(second.endpoint(), 3);
    assert_eq!(ring.pop(), None);
}

#[test]
fn a_full_event_ring_does_not_overwrite() {
    let mut ring = EventRing::new(2).expect("ring");
    assert!(ring.push_event(Trb::port_status_change(1)).is_ok());
    assert!(ring.push_event(Trb::port_status_change(2)).is_ok());
    assert_eq!(
        ring.push_event(Trb::port_status_change(3)),
        Err(RingError::Full)
    );
    assert!(ring.pop().is_some());
    assert!(ring.pop().is_some());
    assert_eq!(ring.pop(), None);
    assert!(ring.push_event(Trb::port_status_change(3)).is_ok());
}

#[test]
fn the_event_ring_flips_both_cycles_on_wrap() {
    let mut ring = EventRing::new(2).expect("ring");
    assert_eq!(ring.push_event(Trb::port_status_change(1)), Ok(0));
    assert_eq!(ring.push_event(Trb::port_status_change(2)), Ok(1));
    assert_eq!(ring.producer(), 0);
    assert_eq!(ring.consumer(), 0);

    let (first_index, _) = ring.pop().expect("first");
    let (second_index, _) = ring.pop().expect("second");
    assert_eq!((first_index, second_index), (0, 1));
    assert_eq!(ring.consumer(), 0);

    assert_eq!(ring.push_event(Trb::port_status_change(3)), Ok(0));
    assert_eq!(
        ring.slot(0).map(|trb| trb.cycle()),
        Some(false),
        "the producer cycle flipped with the wrap"
    );
    let (index, event) = ring.pop().expect("wrapped event");
    assert_eq!(index, 0);
    assert!(!event.cycle());
}
