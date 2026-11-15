// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host tests for the descriptor and ring model.

use super::*;
use raven_proto_net::ring::DESC_CHECKSUM_OK;

fn rx_status(length: u32, flags: u32) -> RxStatus {
    RxStatus::from_raw((length << RX_STATUS_BUFLEN_SHIFT) | flags)
}

#[test]
fn an_rx_status_decodes_its_length_and_ownership() {
    let status = rx_status(64, RX_STATUS_OWN | RX_STATUS_SOP | RX_STATUS_EOP);
    assert_eq!(status.length(), 64);
    assert!(status.own());
    assert!(status.sop());
    assert!(status.eop());
    assert!(!status.errors().any());
}

#[test]
fn a_complete_rx_frame_trims_the_two_byte_alignment() {
    let status = rx_status(64, RX_STATUS_SOP | RX_STATUS_EOP);
    let frame = status.frame().expect("valid frame");
    assert_eq!(frame.length, 62);
    assert_eq!(frame.raw_length, 64);
}

#[test]
fn an_owned_rx_descriptor_is_busy() {
    let status = rx_status(64, RX_STATUS_OWN | RX_STATUS_SOP | RX_STATUS_EOP);
    assert_eq!(status.frame(), Err(RxError::Busy));
}

#[test]
fn every_rx_error_bit_is_reported() {
    let bits = [
        RX_STATUS_OVRUN_ERR,
        RX_STATUS_CRC_ERR,
        RX_STATUS_RX_ERR,
        RX_STATUS_FRAME_ERR,
        RX_STATUS_LEN_ERR,
    ];
    for bit in bits {
        let status = rx_status(64, RX_STATUS_SOP | RX_STATUS_EOP | bit);
        assert!(status.errors().any());
        assert!(matches!(status.frame(), Err(RxError::Device(_))));
    }
}

#[test]
fn rx_errors_decode_their_individual_names() {
    let errors = RxErrors::from_bits(RX_STATUS_ALL_ERRS as u8);
    assert!(errors.overrun());
    assert!(errors.crc());
    assert!(errors.receive());
    assert!(errors.frame());
    assert!(errors.length());
    assert_eq!(errors.bits(), RX_STATUS_ALL_ERRS as u8);
}

#[test]
fn an_rx_frame_missing_its_boundaries_is_rejected() {
    assert_eq!(
        rx_status(64, RX_STATUS_EOP).frame(),
        Err(RxError::MissingStart)
    );
    assert_eq!(
        rx_status(64, RX_STATUS_SOP).frame(),
        Err(RxError::MissingEnd)
    );
}

#[test]
fn an_rx_frame_shorter_than_the_alignment_is_rejected() {
    assert_eq!(
        rx_status(0, RX_STATUS_SOP | RX_STATUS_EOP).frame(),
        Err(RxError::TooShort(0))
    );
    assert_eq!(
        rx_status(1, RX_STATUS_SOP | RX_STATUS_EOP).frame(),
        Err(RxError::TooShort(1))
    );
}

#[test]
fn an_rx_frame_larger_than_the_packet_buffer_is_rejected() {
    assert_eq!(
        rx_status((BUFFER_LEN + 1) as u32, RX_STATUS_SOP | RX_STATUS_EOP).frame(),
        Err(RxError::Oversize(BUFFER_LEN + 1))
    );
}

#[test]
fn a_tx_segment_sets_sop_eop_crc_and_the_queue_tag() {
    let first = TxStatus::segment(100, 0x15, true, false).expect("first segment");
    assert_eq!(first.length(), 100);
    assert!(first.sop());
    assert!(!first.eop());
    assert!(first.crc());
    assert_eq!(first.queue_tag(), 0x15);
    assert!(!first.own());

    let last = TxStatus::segment(100, 0x15, false, true).expect("last segment");
    assert!(!last.sop());
    assert!(last.eop());
}

#[test]
fn a_tx_segment_rejects_zero_and_oversized_lengths() {
    assert_eq!(
        TxStatus::segment(0, 0, true, true),
        Err(TxError::BadLength(0))
    );
    assert_eq!(
        TxStatus::segment(BUFFER_LEN + 1, 0, true, true),
        Err(TxError::BadLength(BUFFER_LEN + 1))
    );
}

#[test]
fn a_tx_segment_rejects_a_queue_tag_wider_than_six_bits() {
    assert_eq!(
        TxStatus::segment(64, TX_STATUS_QTAG_MASK as u8 + 1, true, true),
        Err(TxError::BadQueueTag(TX_STATUS_QTAG_MASK as u8 + 1))
    );
}

#[test]
fn the_tx_helpers_build_the_pi4_flag_sets() {
    let first = TxStatus::first(64).expect("first");
    assert_eq!(first.queue_tag(), TX_STATUS_QTAG_MASK as u8);
    assert_eq!(first.raw() & TX_STATUS_QTAG, TX_STATUS_QTAG);
    let last = TxStatus::last(64).expect("last");
    assert!(last.eop());
    let single = TxStatus::single(64).expect("single");
    assert!(single.sop() && single.eop());
}

#[test]
fn a_tx_status_reports_device_ownership() {
    let status = TxStatus::single(64).expect("status");
    assert!(!status.own());
    let in_flight = TxStatus::from_raw(status.raw() | TX_STATUS_OWN);
    assert!(in_flight.own());
}

#[test]
fn descriptors_encode_status_and_a_64_bit_bus_address() {
    let rx = RxDescriptor::new(0x1_2345_6789, rx_status(64, RX_STATUS_SOP | RX_STATUS_EOP));
    assert_eq!(
        rx.registers(),
        [
            (64 << RX_STATUS_BUFLEN_SHIFT) | RX_STATUS_SOP | RX_STATUS_EOP,
            0x2345_6789,
            0x1,
        ]
    );
    let tx = TxDescriptor::new(0xdead_beef, TxStatus::single(64).expect("status"));
    assert_eq!(tx.registers()[1], 0xdead_beef);
    assert_eq!(tx.registers()[2], 0);
}

#[test]
fn a_ring_rejects_zero_and_overflowing_counts() {
    assert_eq!(RingIndices::new(0), Err(RingError::ZeroCount));
    assert_eq!(
        RingIndices::new(MAX_RING_ENTRIES + 1),
        Err(RingError::TooManyEntries)
    );
    assert!(RingIndices::new(MAX_RING_ENTRIES).is_ok());
}

#[test]
fn a_new_ring_is_empty_and_fully_free() {
    let ring = RingIndices::new(256).expect("ring");
    assert!(ring.is_empty());
    assert!(!ring.is_full());
    assert_eq!(ring.used(), Ok(0));
    assert_eq!(ring.free(), Ok(256));
    assert_eq!(ring.producer_slot(), 0);
    assert_eq!(ring.consumer_slot(), 0);
}

#[test]
fn advancing_the_producer_moves_the_write_slot() {
    let mut ring = RingIndices::new(4).expect("ring");
    ring.advance_producer(1).expect("push");
    assert_eq!(ring.producer(), 1);
    assert_eq!(ring.producer_slot(), 1);
    assert_eq!(ring.used(), Ok(1));
}

#[test]
fn a_ring_wraps_its_slot_at_the_descriptor_count() {
    let mut ring = RingIndices::new(256).expect("ring");
    ring.advance_producer(255).expect("push");
    assert_eq!(ring.producer_slot(), 255);
    ring.advance_producer(1).expect("push");
    assert_eq!(ring.producer(), 256);
    assert_eq!(ring.producer_slot(), 0);
    assert_eq!(ring.hardware_producer(), 256);
}

#[test]
fn the_hardware_counter_wraps_at_sixteen_bits() {
    let ring = RingIndices::from_counters(256, 0x1_0000, 0xffff).expect("wrapped ring");
    assert_eq!(ring.hardware_producer(), 0);
    assert_eq!(ring.hardware_consumer(), 0xffff);
    assert_eq!(ring.producer(), 0x1_0000);
    assert_eq!(ring.used(), Ok(1));
    assert_eq!(ring.producer_slot(), 0);
}

#[test]
fn ring_full_and_empty_are_exact() {
    let mut ring = RingIndices::new(2).expect("ring");
    ring.advance_producer(2).expect("fill");
    assert!(ring.is_full());
    assert_eq!(ring.advance_producer(1), Err(RingError::Full));
    ring.advance_consumer(2).expect("drain");
    assert!(ring.is_empty());
    assert_eq!(ring.advance_consumer(1), Err(RingError::Empty));
}

#[test]
fn a_consumer_cannot_pass_the_producer() {
    let mut ring = RingIndices::new(8).expect("ring");
    ring.advance_producer(3).expect("push");
    ring.advance_consumer(2).expect("pop");
    assert_eq!(ring.used(), Ok(1));
    assert_eq!(ring.advance_consumer(2), Err(RingError::Empty));
}

#[test]
fn hardware_delta_uses_wrapping_arithmetic() {
    assert_eq!(hardware_delta(5, 0), 5);
    assert_eq!(hardware_delta(0, 5), 0xfffb);
    assert_eq!(hardware_delta(0x0001, 0xfffe), 3);
}

#[test]
fn a_rebuilt_ring_rejects_an_impossible_distance() {
    assert!(RingIndices::from_counters(8, 9, 0).is_err());
    let ring = RingIndices::from_counters(8, 9, 8).expect("wrapped ring");
    assert_eq!(ring.used(), Ok(1));
    assert_eq!(ring.producer_slot(), 1);
}

#[test]
fn the_pi4_ring_setup_matches_the_donor_values() {
    let setup = RingSetup::pi4_default();
    assert_eq!(setup.desc_count, 256);
    assert_eq!(setup.buffer_len, 2048);
    assert_eq!(setup.buf_size, 0x0100_0800);
    assert_eq!(setup.end_address, 767);
    assert_eq!(setup.xon_xoff, 0x0005_0010);
    assert_eq!(setup.tx_threshold, 10);
    assert_eq!(setup.rx_threshold, 10);
    assert_eq!(setup.rx_timeout_ticks, 7);
}

#[test]
fn a_valid_rx_status_becomes_a_b39_first_last_descriptor() {
    let status = rx_status(64, RX_STATUS_SOP | RX_STATUS_EOP);
    let descriptor = netd_rx_descriptor(status, 0x2000);
    assert_eq!(descriptor.addr, 0x2000);
    assert_eq!(descriptor.len, 62);
    assert_eq!(descriptor.flags, DESC_FIRST | DESC_LAST);
    assert!(descriptor.is_first());
    assert!(descriptor.is_last());
}

#[test]
fn a_malformed_rx_status_becomes_a_b39_error_descriptor() {
    let busy = rx_status(64, RX_STATUS_OWN | RX_STATUS_SOP | RX_STATUS_EOP);
    let descriptor = netd_rx_descriptor(busy, 0);
    assert_eq!(descriptor.len, 0);
    assert_eq!(descriptor.flags & DESC_ERROR, DESC_ERROR);

    let short = rx_status(1, RX_STATUS_SOP | RX_STATUS_EOP);
    let descriptor = netd_rx_descriptor(short, 0);
    assert_eq!(descriptor.flags & DESC_ERROR, DESC_ERROR);
}

#[test]
fn a_b39_tx_descriptor_becomes_a_genet_tx_status() {
    let descriptor = Desc::new(0, 100, DESC_FIRST | DESC_LAST | DESC_CHECKSUM_OK);
    let status = tx_status_from_netd(descriptor).expect("status");
    assert_eq!(status.length(), 100);
    assert!(status.sop() && status.eop());
    assert_eq!(status.queue_tag(), TX_STATUS_QTAG_MASK as u8);
}

#[test]
fn a_b39_tx_descriptor_with_error_or_unknown_flags_is_rejected() {
    assert_eq!(
        tx_status_from_netd(Desc::new(0, 100, DESC_FIRST | DESC_ERROR)),
        Err(TxError::BadFlags(DESC_FIRST | DESC_ERROR))
    );
    assert_eq!(
        tx_status_from_netd(Desc::new(0, 100, 1 << 31)),
        Err(TxError::BadFlags(1 << 31))
    );
}
