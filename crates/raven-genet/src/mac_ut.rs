// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host tests for MAC address handling and the source resolver.

use super::*;
use raven_abi::errno::Errno;
use raven_plat::mailbox::{FakeMailbox, Mailbox};

const SAMPLE: MacAddress = MacAddress::new([0xd8, 0x3a, 0xdd, 0x12, 0x34, 0x56]);
const OTHER: MacAddress = MacAddress::new([0x02, 0x00, 0x00, 0x00, 0x00, 0x42]);

#[test]
fn a_unicast_nonzero_address_is_valid() {
    assert!(SAMPLE.is_valid());
    assert!(OTHER.is_valid());
}

#[test]
fn zero_broadcast_and_multicast_addresses_are_invalid() {
    assert!(!MacAddress::new([0; 6]).is_valid());
    assert!(!MacAddress::new([0xff; 6]).is_valid());
    assert!(!MacAddress::new([0x01, 0, 0, 0, 0, 1]).is_valid());
    assert!(!MacAddress::new([0x03, 0, 0, 0, 0, 1]).is_valid());
}

#[test]
fn the_display_form_is_the_usual_dotted_hex() {
    assert_eq!(SAMPLE.to_string(), "d8:3a:dd:12:34:56");
}

#[test]
fn the_mailbox_words_decode_in_little_endian_order() {
    let words = [
        u32::from_le_bytes([0xd8, 0x3a, 0xdd, 0x12]),
        u32::from_le_bytes([0x34, 0x56, 0, 0]),
    ];
    assert_eq!(MacAddress::from_mailbox_words(words), SAMPLE);
}

#[test]
fn the_umac_words_are_big_endian_halves() {
    assert_eq!(SAMPLE.umac_words(), (0xd83a_dd12, 0x3456));
    assert_eq!(OTHER.umac_words(), (0x0200_0000, 0x0042));
}

#[test]
fn a_device_tree_address_wins_without_asking_the_mailbox() {
    let mut resolver = MacResolver::new(Some(SAMPLE));
    assert_eq!(
        resolver.next_action(),
        MacAction::Done(MacSelection {
            address: SAMPLE,
            origin: MacOrigin::DeviceTree,
        })
    );
    assert_eq!(
        resolver.next_action(),
        MacAction::Done(MacSelection {
            address: SAMPLE,
            origin: MacOrigin::DeviceTree,
        })
    );
}

#[test]
fn an_invalid_device_tree_address_falls_through_to_the_mailbox() {
    let mut resolver = MacResolver::new(Some(MacAddress::new([0; 6])));
    assert_eq!(resolver.next_action(), MacAction::QueryMailbox);
    assert_eq!(
        resolver.mailbox_result(Some(SAMPLE)),
        MacAction::Done(MacSelection {
            address: SAMPLE,
            origin: MacOrigin::Mailbox,
        })
    );
}

#[test]
fn an_absent_mailbox_result_falls_through_to_hardware() {
    let mut resolver = MacResolver::new(None);
    assert_eq!(resolver.next_action(), MacAction::QueryMailbox);
    assert_eq!(resolver.mailbox_result(None), MacAction::ReadHardware);
    assert_eq!(
        resolver.hardware_result(Some(OTHER)),
        MacAction::Done(MacSelection {
            address: OTHER,
            origin: MacOrigin::Hardware,
        })
    );
}

#[test]
fn all_sources_absent_or_invalid_is_a_clean_failure() {
    let mut resolver = MacResolver::new(None);
    assert_eq!(resolver.next_action(), MacAction::QueryMailbox);
    assert_eq!(
        resolver.mailbox_result(Some(MacAddress::new([0xff; 6]))),
        MacAction::ReadHardware
    );
    assert_eq!(
        resolver.hardware_result(Some(MacAddress::new([0; 6]))),
        MacAction::Failed(MacError::NoSource)
    );
    assert_eq!(
        resolver.next_action(),
        MacAction::Failed(MacError::NoSource)
    );
    assert_eq!(resolver.selection(), None);
}

#[test]
fn an_unrequested_mailbox_or_hardware_result_is_rejected() {
    let mut resolver = MacResolver::new(None);
    assert_eq!(
        resolver.mailbox_result(Some(SAMPLE)),
        MacAction::Failed(MacError::UnexpectedSource)
    );
    assert_eq!(
        resolver.hardware_result(Some(SAMPLE)),
        MacAction::Failed(MacError::UnexpectedSource)
    );
}

#[test]
fn the_fake_mailbox_returns_the_board_mac() {
    let mut mailbox = FakeMailbox::new();
    assert_eq!(
        mac_from_mailbox(&mut mailbox),
        Ok(MacAddress::new([0x02, 0x00, 0x00, 0x00, 0x00, 0x01]))
    );
}

struct FailingMailbox;

impl Mailbox for FailingMailbox {
    fn query(
        &mut self,
        _tag: u32,
        _request: &[u32],
        _response: &mut [u32],
    ) -> Result<usize, Errno> {
        Err(Errno::ENODEV)
    }
}

#[test]
fn a_failing_mailbox_is_reported_as_a_source_failure() {
    assert_eq!(
        mac_from_mailbox(&mut FailingMailbox),
        Err(MacError::Mailbox)
    );
}

struct ShortMailbox;

impl Mailbox for ShortMailbox {
    fn query(
        &mut self,
        _tag: u32,
        _request: &[u32],
        _response: &mut [u32],
    ) -> Result<usize, Errno> {
        Ok(1)
    }
}

#[test]
fn a_short_mailbox_response_is_rejected() {
    assert_eq!(mac_from_mailbox(&mut ShortMailbox), Err(MacError::Mailbox));
}

struct MulticastMailbox;

impl Mailbox for MulticastMailbox {
    fn query(&mut self, _tag: u32, _request: &[u32], response: &mut [u32]) -> Result<usize, Errno> {
        response[0] = 0x0000_0001;
        response[1] = 0;
        Ok(2)
    }
}

#[test]
fn a_multicast_mailbox_address_is_rejected() {
    assert_eq!(
        mac_from_mailbox(&mut MulticastMailbox),
        Err(MacError::InvalidAddress)
    );
}
