// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Assertions for the descriptor ring.

use super::*;

fn ring(entries: u16) -> Ring {
    Ring::new(entries).expect("ring")
}

#[test]
fn invalid_capacities_are_rejected() {
    assert_eq!(Ring::new(0).err(), Some(RingError::ZeroEntries));
    assert_eq!(Ring::new(3).err(), Some(RingError::NotPowerOfTwo(3)));
    assert_eq!(
        Ring::new(MAX_ENTRIES + 1).err(),
        Some(RingError::TooManyEntries(MAX_ENTRIES + 1))
    );
}

#[test]
fn a_ring_fills_and_reports_full() {
    let mut ring = ring(4);
    assert_eq!(ring.capacity(), 4);
    assert!(ring.is_empty());
    for index in 0..4u16 {
        assert_eq!(
            ring.submit(Descriptor::new(u64::from(index) * 0x1000, 0x100)),
            Ok(index)
        );
    }
    assert!(ring.is_full());
    assert_eq!(ring.len(), 4);
    assert_eq!(ring.submit(Descriptor::new(0, 1)), Err(RingError::Full));
}

#[test]
fn completions_are_fifo_and_transfer_ownership() {
    let mut ring = ring(4);
    let descriptor = Descriptor::new(0x1000, 0x100).with_flags(DescFlags {
        first: true,
        last: false,
        error: false,
    });
    assert_eq!(ring.submit(descriptor), Ok(0));
    assert_eq!(ring.peek().map(|entry| entry.owner), Some(Owner::Device));

    let (index, completed) = ring.complete().expect("completion");
    assert_eq!(index, 0);
    assert_eq!(completed.owner, Owner::Host);
    assert!(!completed.flags.last);
    assert!(ring.is_empty());
    assert_eq!(ring.complete(), None);
}

#[test]
fn the_ring_wraps_over_many_cycles() {
    let mut ring = ring(2);
    let mut indices = [0u16; 6];
    for (cycle, slot) in indices.iter_mut().enumerate() {
        let value = u64::try_from(cycle).expect("cycle");
        *slot = ring.submit(Descriptor::new(value, 1)).expect("submit");
        let (index, descriptor) = ring.complete().expect("complete");
        assert_eq!(index, *slot);
        assert_eq!(descriptor.buffer, value);
    }
    assert_eq!(indices, [0, 1, 0, 1, 0, 1]);
    assert!(ring.is_empty());
}

#[test]
fn a_queued_ring_reuses_freed_slots_after_wrap() {
    let mut ring = ring(2);
    assert_eq!(ring.submit(Descriptor::new(1, 1)), Ok(0));
    assert_eq!(ring.submit(Descriptor::new(2, 1)), Ok(1));
    assert_eq!(ring.tail(), 0);
    let (first, _) = ring.complete().expect("first");
    assert_eq!(first, 0);
    assert_eq!(ring.submit(Descriptor::new(3, 1)), Ok(0));
    let (second, descriptor) = ring.complete().expect("second");
    assert_eq!(second, 1);
    assert_eq!(descriptor.buffer, 2);
    let (third, descriptor) = ring.complete().expect("third");
    assert_eq!(third, 0);
    assert_eq!(descriptor.buffer, 3);
}

#[test]
fn out_of_range_slots_are_not_visible() {
    let ring = ring(4);
    assert_eq!(ring.descriptor(4), None);
    assert_eq!(
        ring.descriptor(0).map(|entry| entry.owner),
        Some(Owner::Host)
    );
}
