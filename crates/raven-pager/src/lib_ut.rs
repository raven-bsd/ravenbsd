// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host tests for the pager page state and packet model.

use super::*;

#[test]
fn new_object_is_all_absent() {
    let object = PagerObject::new(4).expect("object");
    assert_eq!(object.len(), 4);
    for page in 0..4 {
        assert_eq!(object.state(page), Some(PageState::Absent));
        assert!(!object.ready(page));
    }
    assert_eq!(object.state(4), None);
    assert!(PagerObject::new(0).is_none());
}

#[test]
fn first_fault_owns_the_request_and_later_faults_wait() {
    let mut object = PagerObject::new(1).expect("object");
    assert_eq!(object.begin_page_in(0), PageIn::Send);
    assert_eq!(object.state(0), Some(PageState::InFlight));
    assert_eq!(object.begin_page_in(0), PageIn::Wait);
    assert_eq!(object.in_flight(), 1);
}

#[test]
fn supply_makes_the_page_ready_and_is_not_repeated() {
    let mut object = PagerObject::new(1).expect("object");
    assert_eq!(object.begin_page_in(0), PageIn::Send);
    assert!(object.supply(0));
    assert_eq!(object.state(0), Some(PageState::Present));
    assert!(object.ready(0));
    assert_eq!(object.begin_page_in(0), PageIn::Present);
    assert!(!object.supply(0));
    assert_eq!(object.in_flight(), 0);
}

#[test]
fn failure_is_terminal_and_wakes_a_waiting_fault() {
    let mut object = PagerObject::new(1).expect("object");
    assert_eq!(object.begin_page_in(0), PageIn::Send);
    assert!(object.fail(0));
    assert_eq!(object.begin_page_in(0), PageIn::Failed);
    assert!(!object.supply(0));
    assert_eq!(object.state(0), Some(PageState::Failed));
}

#[test]
fn dirty_writeback_completes() {
    let mut object = PagerObject::new(1).expect("object");
    assert!(object.supply(0));
    assert!(object.mark_dirty(0));
    assert_eq!(object.state(0), Some(PageState::Dirty));
    assert_eq!(object.dirty_pages(), 1);
    assert_eq!(object.begin_page_out(0), PageOut::Send);
    assert_eq!(object.state(0), Some(PageState::Writing));
    assert_eq!(object.begin_page_out(0), PageOut::Wait);
    assert!(object.page_out_done(0));
    assert_eq!(object.state(0), Some(PageState::Present));
    assert_eq!(object.dirty_pages(), 0);
    assert_eq!(object.begin_page_out(0), PageOut::Clean);
}

#[test]
fn a_write_during_writeback_keeps_the_page_dirty() {
    let mut object = PagerObject::new(1).expect("object");
    assert!(object.supply(0));
    assert!(object.mark_dirty(0));
    assert_eq!(object.begin_page_out(0), PageOut::Send);
    // A writer touches the page while the pager is writing it back.
    assert!(object.mark_dirty(0));
    assert_eq!(object.state(0), Some(PageState::Dirty));
    assert!(!object.page_out_done(0));
    assert_eq!(object.state(0), Some(PageState::Dirty));
    assert_eq!(object.begin_page_out(0), PageOut::Send);
}

#[test]
fn failed_writeback_retries() {
    let mut object = PagerObject::new(1).expect("object");
    assert!(object.supply(0));
    assert!(object.mark_dirty(0));
    assert_eq!(object.begin_page_out(0), PageOut::Send);
    assert!(object.page_out_fail(0));
    assert_eq!(object.state(0), Some(PageState::Dirty));
    assert_eq!(object.begin_page_out(0), PageOut::Send);
}

#[test]
fn present_object_is_eager() {
    let object = PagerObject::present(3).expect("object");
    assert_eq!(object.len(), 3);
    for page in 0..3 {
        assert!(object.ready(page));
    }
}

#[test]
fn packet_round_trips() {
    for packet in [
        PagerPacket::page_in(7, 0x3000),
        PagerPacket::page_out(9, 0x5000),
    ] {
        let mut bytes = [0u8; PACKET_SIZE];
        packet.encode(&mut bytes).expect("encode");
        assert_eq!(PagerPacket::decode(&bytes), Ok(packet));
    }
}

#[test]
fn short_packet_is_rejected() {
    let mut bytes = [0u8; PACKET_SIZE - 1];
    assert_eq!(
        PagerPacket::page_in(1, 0).encode(&mut bytes),
        Err(PacketError::TooShort)
    );
    assert_eq!(PagerPacket::decode(&bytes), Err(PacketError::TooShort));
}

#[test]
fn bad_magic_kind_reserved_and_range_are_rejected() {
    let mut bytes = [0u8; PACKET_SIZE];
    PagerPacket::page_in(1, 0)
        .encode(&mut bytes)
        .expect("encode");
    let mut bad = bytes;
    bad[0] ^= 0xff;
    assert_eq!(PagerPacket::decode(&bad), Err(PacketError::BadMagic));

    let mut bad = bytes;
    bad[4] = 3;
    assert_eq!(PagerPacket::decode(&bad), Err(PacketError::BadKind));

    let mut bad = bytes;
    bad[6] = 1;
    assert_eq!(PagerPacket::decode(&bad), Err(PacketError::BadReserved));

    let mut bad = bytes;
    bad[16] = 1;
    assert_eq!(PagerPacket::decode(&bad), Err(PacketError::BadRange));

    let mut bad = bytes;
    bad[24] = 0;
    bad[25] = 0;
    assert_eq!(PagerPacket::decode(&bad), Err(PacketError::BadRange));
}

#[test]
fn counts_track_states() {
    let mut object = PagerObject::new(3).expect("object");
    assert_eq!(object.in_flight(), 0);
    assert_eq!(object.dirty_pages(), 0);
    assert_eq!(object.begin_page_in(0), PageIn::Send);
    assert_eq!(object.in_flight(), 1);
    assert!(object.supply(0));
    assert!(object.mark_dirty(0));
    assert_eq!(object.dirty_pages(), 1);
    assert_eq!(object.begin_page_out(0), PageOut::Send);
    assert_eq!(object.in_flight(), 1);
}
