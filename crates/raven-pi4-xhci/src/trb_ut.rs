// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

use super::*;

const SYNTHETIC: &str = include_str!("../../../tests/fixtures/pi4-usb/synthetic-trb.txt");

fn fixture_words() -> Vec<[u32; 4]> {
    SYNTHETIC
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .map(|line| {
            let words: Vec<u32> = line
                .split_whitespace()
                .map(|word| u32::from_str_radix(word, 16).expect("hex word"))
                .collect();
            assert_eq!(words.len(), 4, "fixture line needs four words");
            [words[0], words[1], words[2], words[3]]
        })
        .collect()
}

#[test]
fn words_round_trip() {
    let trb = Trb::from_words([1, 2, 3, 4]);
    assert_eq!(trb.parameter, 0x0000_0002_0000_0001);
    assert_eq!(trb.to_words(), [1, 2, 3, 4]);
}

#[test]
fn type_codes_round_trip() {
    for kind in [
        TrbType::Normal,
        TrbType::SetupStage,
        TrbType::DataStage,
        TrbType::StatusStage,
        TrbType::Link,
        TrbType::EnableSlot,
        TrbType::AddressDevice,
        TrbType::ConfigureEndpoint,
        TrbType::TransferEvent,
        TrbType::CommandCompletion,
        TrbType::PortStatusChange,
    ] {
        assert_eq!(TrbType::from_code(kind.code()), kind);
    }
    assert_eq!(TrbType::from_code(63), TrbType::Other(63));
}

#[test]
fn normal_transfer_encodes_length_and_ioc() {
    let trb = Trb::normal(0x1000, 512, true);
    assert_eq!(trb.kind(), TrbType::Normal);
    assert_eq!(trb.length(), 512);
    assert!(trb.ioc());
    assert!(!trb.chain());
    assert!(!trb.cycle());
    assert!(trb.with_cycle(true).cycle());
    assert!(!trb.with_cycle(false).cycle());
}

#[test]
fn setup_data_and_status_constructors() {
    let setup = Trb::setup(0x1122_3344_5566_7788);
    assert_eq!(setup.kind(), TrbType::SetupStage);
    assert_eq!(setup.parameter, 0x1122_3344_5566_7788);
    assert_eq!(setup.length(), 8);
    let data = Trb::data(0x2000, 64);
    assert_eq!(data.kind(), TrbType::DataStage);
    assert_eq!(data.length(), 64);
    let status = Trb::status(TRB_CHAIN);
    assert_eq!(status.kind(), TrbType::StatusStage);
    assert!(status.chain());
}

#[test]
fn link_trb_carries_toggle_cycle() {
    let link = Trb::link(0x3000, true);
    assert_eq!(link.kind(), TrbType::Link);
    assert_eq!(link.parameter, 0x3000);
    assert!(link.toggle_cycle());
}

#[test]
fn slot_and_endpoint_fields_are_bounded() {
    let trb = Trb::address_device(3, 0x4000);
    assert_eq!(trb.kind(), TrbType::AddressDevice);
    assert_eq!(trb.slot(), 3);
    assert_eq!(trb.with_endpoint(0), None);
    assert_eq!(trb.with_endpoint(32), None);
    assert_eq!(trb.with_endpoint(1).expect("endpoint").endpoint(), 1);
}

#[test]
fn transfer_event_decodes_completion_and_length() {
    let event = Trb::transfer_event(1, 1, CompletionCode::Success, 8);
    assert_eq!(event.kind(), TrbType::TransferEvent);
    assert_eq!(event.slot(), 1);
    assert_eq!(event.endpoint(), 1);
    assert_eq!(event.length(), 8);
    assert_eq!(event.completion(), CompletionCode::Success);
    assert_eq!(
        Trb::transfer_event(2, 2, CompletionCode::Stall, 0).completion(),
        CompletionCode::Stall
    );
}

#[test]
fn port_status_event_carries_the_port() {
    let event = Trb::port_status_change(4);
    assert_eq!(event.kind(), TrbType::PortStatusChange);
    assert_eq!(event.parameter >> 24, 4);
}

#[test]
fn completion_codes_round_trip() {
    for code in [
        CompletionCode::Success,
        CompletionCode::ShortPacket,
        CompletionCode::Stall,
        CompletionCode::Cancelled,
    ] {
        assert_eq!(CompletionCode::from_code(code.code()), code);
    }
    assert_eq!(CompletionCode::from_code(42), CompletionCode::Other(42));
}

#[test]
fn synthetic_fixture_parses_into_the_expected_sequence() {
    let words = fixture_words();
    assert_eq!(words.len(), 4);
    let trbs: Vec<Trb> = words.into_iter().map(Trb::from_words).collect();
    assert_eq!(trbs[0].kind(), TrbType::EnableSlot);
    assert_eq!(trbs[1].kind(), TrbType::AddressDevice);
    assert_eq!(trbs[1].slot(), 1);
    assert_eq!(trbs[2].kind(), TrbType::ConfigureEndpoint);
    assert_eq!(trbs[2].slot(), 1);
    assert_eq!(trbs[3].kind(), TrbType::TransferEvent);
    assert_eq!(trbs[3].slot(), 1);
    assert_eq!(trbs[3].endpoint(), 1);
    assert_eq!(trbs[3].length(), 8);
    assert_eq!(trbs[3].completion(), CompletionCode::Success);
}
