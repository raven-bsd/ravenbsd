// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host tests for the mailbox property codec and fake backend.

use super::*;

fn response(buffer: &mut [u8], tag: u32, values: &[u32]) {
    let size = 24 + values.len() * 4;
    buffer[..size].fill(0);
    buffer[0..4].copy_from_slice(&(size as u32).to_le_bytes());
    buffer[4..8].copy_from_slice(&0x8000_0000u32.to_le_bytes());
    buffer[8..12].copy_from_slice(&tag.to_le_bytes());
    buffer[12..16].copy_from_slice(&((values.len() * 4) as u32).to_le_bytes());
    buffer[16..20].copy_from_slice(&0x8000_0000u32.to_le_bytes());
    for (index, value) in values.iter().enumerate() {
        let at = 20 + index * 4;
        buffer[at..at + 4].copy_from_slice(&value.to_le_bytes());
    }
}

#[test]
fn a_property_request_has_the_expected_layout() {
    let mut buffer = [0u8; 64];
    let size = encode_request(&mut buffer, TAG_GET_CLOCKRATE, &[CLOCK_EMMC2]).expect("encode");
    assert_eq!(size, 28);
    assert_eq!(u32::from_le_bytes(buffer[0..4].try_into().unwrap()), 28);
    assert_eq!(
        u32::from_le_bytes(buffer[8..12].try_into().unwrap()),
        TAG_GET_CLOCKRATE
    );
    assert_eq!(u32::from_le_bytes(buffer[12..16].try_into().unwrap()), 4);
    assert_eq!(
        u32::from_le_bytes(buffer[20..24].try_into().unwrap()),
        CLOCK_EMMC2
    );
    assert_eq!(u32::from_le_bytes(buffer[24..28].try_into().unwrap()), 0);
}

#[test]
fn a_response_round_trips_the_values() {
    let mut buffer = [0u8; 64];
    response(
        &mut buffer,
        TAG_GET_BOARDSERIAL,
        &[0x1234_5678, 0x9abc_def0],
    );
    let mut values = [0u32; 4];
    let count = decode_response(&buffer, TAG_GET_BOARDSERIAL, &mut values).expect("decode");
    assert_eq!(count, 2);
    assert_eq!(values[..2], [0x1234_5678, 0x9abc_def0]);
}

#[test]
fn a_failed_tag_status_is_an_error() {
    let mut buffer = [0u8; 64];
    response(&mut buffer, TAG_GET_BOARD_MODEL, &[7]);
    buffer[16..20].copy_from_slice(&0u32.to_le_bytes());
    let mut values = [0u32; 2];
    assert_eq!(
        decode_response(&buffer, TAG_GET_BOARD_MODEL, &mut values),
        Err(Errno::EIO)
    );
}

#[test]
fn a_response_for_another_tag_is_rejected() {
    let mut buffer = [0u8; 64];
    response(&mut buffer, TAG_GET_BOARD_MODEL, &[7]);
    let mut values = [0u32; 2];
    assert_eq!(
        decode_response(&buffer, TAG_GET_BOARD_REVISION, &mut values),
        Err(Errno::EPROTO)
    );
}

#[test]
fn an_oversized_request_is_rejected() {
    let mut buffer = [0u8; MAX_PROPERTY_BYTES];
    assert_eq!(
        encode_request(&mut buffer, TAG_GET_BOARD_MODEL, &[0; MAX_TAG_VALUES + 1]),
        Err(Errno::E2BIG)
    );
    assert_eq!(
        encode_request(&mut [0u8; 8], TAG_GET_BOARD_MODEL, &[]),
        Err(Errno::E2BIG)
    );
}

#[test]
fn the_fake_mailbox_returns_board_clock_and_power_values() {
    let mut mailbox = FakeMailbox::new();
    let mut values = [0u32; 4];
    assert_eq!(mailbox.query(TAG_GET_BOARD_MODEL, &[], &mut values), Ok(1));
    assert_eq!(values[0], BoardInfo::fake().model);
    assert_eq!(mailbox.query(TAG_GET_BOARDSERIAL, &[], &mut values), Ok(2));
    assert_eq!(
        u64::from(values[0]) | (u64::from(values[1]) << 32),
        BoardInfo::fake().serial
    );
    assert_eq!(
        mailbox.query(TAG_GET_CLOCKRATE, &[CLOCK_EMMC2], &mut values),
        Ok(2)
    );
    assert_eq!(values, [CLOCK_EMMC2, 50_000_000, values[2], values[3]]);
    assert_eq!(
        mailbox.query(TAG_GET_POWERSTATE, &[POWER_SDCARD], &mut values),
        Ok(2)
    );
    assert_eq!(values[..2], [POWER_SDCARD, 1]);
    assert_eq!(
        mailbox.query(TAG_GET_FIRMWAREREV, &[], &mut values),
        Err(Errno::ENODEV)
    );
}
