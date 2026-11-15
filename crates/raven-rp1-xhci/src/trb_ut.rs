// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Assertions for TRB encoding and decoding.

use super::*;

#[test]
fn a_normal_trb_encodes_its_fields() {
    let trb = Trb::normal(0x1000, 0x20, true)
        .with_cycle(true)
        .with_endpoint(1)
        .expect("endpoint");
    assert_eq!(trb.parameter, 0x1000);
    assert_eq!(trb.status, 0x20);
    assert_eq!(trb.control, 0x0001_0421);
    assert_eq!(trb.kind(), TrbType::Normal);
    assert!(trb.cycle());
    assert!(trb.ioc());
    assert!(!trb.chain());
    assert_eq!(trb.endpoint(), 1);
    assert_eq!(trb.transfer_length(), 0x20);
}

#[test]
fn a_link_trb_carries_the_toggle_cycle_bit() {
    let trb = Trb::link(0x2000, true);
    assert_eq!(trb.control, 0x1802);
    assert_eq!(trb.kind(), TrbType::Link);
    assert_eq!(trb.parameter, 0x2000);
    assert_eq!(Trb::link(0x2000, false).control, 0x1800);
}

#[test]
fn command_trbs_encode_the_slot() {
    let enable = Trb::enable_slot();
    assert_eq!(enable.control, 0x2400);
    assert_eq!(enable.kind(), TrbType::EnableSlot);

    let address = Trb::address_device(3, 0x3000).expect("slot");
    assert_eq!(address.control, 0x0300_2C00);
    assert_eq!(address.parameter, 0x3000);
    assert_eq!(address.slot(), 3);
    assert_eq!(address.kind(), TrbType::AddressDevice);

    let configure = Trb::configure_endpoint(2, 0x4000).expect("slot");
    assert_eq!(configure.kind(), TrbType::ConfigureEndpoint);
    assert_eq!(configure.slot(), 2);
}

#[test]
fn event_trbs_decode_slot_endpoint_and_completion() {
    let event = Trb::transfer_event(3, 2, 1, 0x4000).expect("event");
    assert_eq!(event.status, 0x0100_0000);
    assert_eq!(event.control, 0x0302_8000);
    assert_eq!(event.kind(), TrbType::TransferEvent);
    assert_eq!(event.slot(), 3);
    assert_eq!(event.endpoint(), 2);
    assert_eq!(event.completion_code(), 1);

    let completion = Trb::command_completion(3, 1, 0x5000).expect("completion");
    assert_eq!(completion.kind(), TrbType::CommandCompletion);
    assert_eq!(completion.control, 0x0300_8400);
    assert_eq!(completion.slot(), 3);
    assert_eq!(completion.completion_code(), 1);
    assert_eq!(completion.parameter, 0x5000);

    let port = Trb::port_status_change(2);
    assert_eq!(port.kind(), TrbType::PortStatusChange);
    assert_eq!(port.control, 0x8800);
    assert_eq!(port.parameter, 0x0200_0000);
}

#[test]
fn words_round_trip() {
    let trb = Trb::normal(0x1234_5678_9ABC_DEF0, 0x4321, true)
        .with_cycle(true)
        .with_endpoint(31)
        .expect("endpoint");
    let words = trb.to_words();
    assert_eq!(Trb::from_words(words), trb);
    assert_eq!(words[0], 0x9ABC_DEF0);
    assert_eq!(words[1], 0x1234_5678);
}

#[test]
fn invalid_slots_and_endpoints_are_rejected() {
    assert!(Trb::address_device(0, 0).is_none());
    assert!(Trb::configure_endpoint(0, 0).is_none());
    assert!(Trb::transfer_event(0, 1, 0, 0).is_none());
    assert!(Trb::transfer_event(1, 0, 0, 0).is_none());
    assert!(Trb::transfer_event(1, 32, 0, 0).is_none());
    assert!(Trb::normal(0, 0, false).with_endpoint(0).is_none());
    assert!(Trb::normal(0, 0, false).with_endpoint(32).is_none());
}

#[test]
fn the_type_codes_round_trip() {
    let types = [
        TrbType::Normal,
        TrbType::SetupStage,
        TrbType::DataStage,
        TrbType::StatusStage,
        TrbType::Link,
        TrbType::EnableSlot,
        TrbType::DisableSlot,
        TrbType::AddressDevice,
        TrbType::ConfigureEndpoint,
        TrbType::ResetEndpoint,
        TrbType::StopEndpoint,
        TrbType::SetTrDequeue,
        TrbType::ResetDevice,
        TrbType::TransferEvent,
        TrbType::CommandCompletion,
        TrbType::PortStatusChange,
    ];
    for kind in types {
        assert_eq!(TrbType::from_code(kind.code()), kind);
    }
    assert_eq!(TrbType::from_code(0x3F), TrbType::Other(0x3F));
}

#[test]
fn the_cycle_bit_can_be_cleared() {
    let trb = Trb::enable_slot().with_cycle(true);
    assert!(trb.cycle());
    assert!(!trb.with_cycle(false).cycle());
}
