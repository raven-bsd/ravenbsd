// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host tests for response kinds, card status, OCR, CID and CSD parsing.

use super::*;

#[test]
fn response_kinds_encode_the_sdhci_type_field() {
    assert_eq!(ResponseKind::None.register_bits(), 0b00);
    assert_eq!(ResponseKind::R2.register_bits(), 0b01);
    assert_eq!(ResponseKind::R1.register_bits(), 0b10);
    assert_eq!(ResponseKind::R3.register_bits(), 0b10);
    assert_eq!(ResponseKind::R1b.register_bits(), 0b11);
    assert!(ResponseKind::R2.crc_check());
    assert!(ResponseKind::R1.crc_check());
    assert!(!ResponseKind::R3.crc_check());
    assert!(!ResponseKind::None.crc_check());
    assert!(ResponseKind::R2.is_long());
    assert!(!ResponseKind::R1b.is_long());
}

#[test]
fn raw_long_addresses_bits_across_the_four_registers() {
    let raw = RawLong::from_registers([0x1122_3344, 0x5566_7788, 0x99aa_bbcc, 0xddee_ff00]);
    assert_eq!(raw.bits(31, 0), 0x1122_3344);
    assert_eq!(raw.bits(63, 32), 0x5566_7788);
    assert_eq!(raw.bits(127, 96), 0xddee_ff00);
    assert_eq!(raw.bits(47, 16), 0x7788_1122);
    assert_eq!(raw.word(0), Some(0x1122_3344));
    assert_eq!(raw.word(3), Some(0xddee_ff00));
    assert_eq!(raw.word(4), None);
    assert_eq!(raw.bits(0, 1), 0);
    assert_eq!(raw.bits(128, 127), 0);
}

#[test]
fn card_status_decodes_state_and_flags() {
    let raw = (1 << 8) | (1 << 5) | (4 << 9);
    let status = CardStatus::from_raw(raw);
    assert!(status.ready_for_data());
    assert!(status.app_cmd());
    assert!(!status.command_error());
    assert_eq!(status.state(), Some(CurrentState::Transfer));
    assert_eq!(status.raw(), raw);
    assert!(CardStatus::from_raw(1 << 25).is_locked());
}

#[test]
fn card_status_error_bits_are_not_ignored() {
    assert!(CardStatus::from_raw(1 << 22).command_error());
    assert!(CardStatus::from_raw(1 << 31).command_error());
    assert!(CardStatus::from_raw(1 << 16).command_error());
    assert!(CardStatus::from_raw(1 << 13).command_error());
    assert!(CardStatus::from_raw(1 << 3).command_error());
    assert!(!CardStatus::from_raw(1 << 25).command_error());
    assert_eq!(CurrentState::from_bits(8), Some(CurrentState::Disconnected));
    assert_eq!(CurrentState::from_bits(9), None);
}

#[test]
fn r1_r3_r6_and_r7_decode_their_fields() {
    let status = CardStatus::from_raw(1 << 8);
    assert_eq!(
        decode(ResponseKind::R1, [1 << 8, 0, 0, 0]),
        Response::R1(status)
    );
    assert_eq!(
        decode(ResponseKind::R1b, [1 << 8, 0, 0, 0]),
        Response::R1(status)
    );

    let ocr = decode(
        ResponseKind::R3,
        [(1 << 31) | (1 << 30) | (0x1ff << 15), 0, 0, 0],
    );
    match ocr {
        Response::R3(ocr) => {
            assert!(ocr.ready());
            assert!(ocr.capacity_status());
            assert_eq!(ocr.voltage_window(), 0x1ff);
        }
        other => panic!("expected R3, got {other:?}"),
    }

    let r6 = decode(ResponseKind::R6, [0x1234_0000 | (1 << 8), 0, 0, 0]);
    match r6 {
        Response::R6 { rca, status } => {
            assert_eq!(rca, 0x1234);
            assert!(status.ready_for_data());
        }
        other => panic!("expected R6, got {other:?}"),
    }

    let r7 = decode(ResponseKind::R7, [0x1aa, 0, 0, 0]);
    match r7 {
        Response::R7(echo) => {
            assert_eq!(echo.voltage_accepted(), 1);
            assert_eq!(echo.check_pattern(), 0xaa);
            assert_eq!(echo.raw(), 0x1aa);
        }
        other => panic!("expected R7, got {other:?}"),
    }

    assert_eq!(decode(ResponseKind::None, [0; 4]), Response::None);
}

#[test]
fn cid_decodes_manufacturer_product_and_serial() {
    let words = [
        0x78 << 24,
        (0x10 << 24) | 0x0012_3456,
        (u32::from(b'B') << 24)
            | (u32::from(b'C') << 16)
            | (u32::from(b'1') << 8)
            | u32::from(b'2'),
        (0x03 << 24) | (0x5344 << 8) | u32::from(b'A'),
    ];
    let cid = Cid::from_raw(RawLong::from_registers(words));
    assert_eq!(cid.manufacturer(), 0x03);
    assert_eq!(cid.oem(), 0x5344);
    assert_eq!(cid.product_name(), *b"ABC12");
    assert_eq!(cid.revision(), 0x10);
    assert_eq!(cid.serial(), 0x1234_5678);
    assert_eq!(cid.raw().registers(), words);
}

#[test]
fn cid_with_an_empty_name_is_still_safe() {
    let cid = Cid::from_raw(RawLong::default());
    assert_eq!(cid.product_name(), [0; 5]);
    assert_eq!(cid.manufacturer(), 0);
    assert_eq!(cid.serial(), 0);
}

fn csd_v2(c_size: u32) -> RawLong {
    let mut words = [0u32; 4];
    words[3] |= 1 << 30;
    words[1] |= (c_size & 0xffff) << 16;
    words[2] |= (c_size >> 16) & 0x3f;
    words[2] |= 9 << 16;
    RawLong::from_registers(words)
}

fn csd_v1(read_bl_len: u32, c_size: u32, c_size_mult: u32) -> RawLong {
    let mut words = [0u32; 4];
    words[1] |= (c_size & 0x3) << 30;
    words[1] |= (c_size_mult & 0x7) << 15;
    words[2] |= (c_size >> 2) & 0x3ff;
    words[2] |= (read_bl_len & 0xf) << 16;
    RawLong::from_registers(words)
}

#[test]
fn csd_version_2_reports_a_sdhc_capacity() {
    let csd = Csd::from_raw(csd_v2(0xffff));
    assert_eq!(csd.structure_version(), 1);
    assert_eq!(csd.read_block_len(), Some(512));
    assert_eq!(csd.capacity_bytes(), Some(32 * 1024 * 1024 * 1024));
}

#[test]
fn csd_version_2_reports_an_sdxc_capacity() {
    let csd = Csd::from_raw(csd_v2(0x3f_ffff));
    assert_eq!(csd.structure_version(), 1);
    assert_eq!(csd.capacity_bytes(), Some(2 * 1024 * 1024 * 1024 * 1024));
}

#[test]
fn csd_version_1_reports_a_legacy_capacity() {
    let csd = Csd::from_raw(csd_v1(9, 0xfff, 7));
    assert_eq!(csd.structure_version(), 0);
    assert_eq!(csd.read_block_len(), Some(512));
    assert_eq!(csd.capacity_bytes(), Some(1024 * 1024 * 1024));
}

#[test]
fn reserved_csd_structures_have_no_capacity() {
    let mut words = [0u32; 4];
    words[3] = 2 << 30;
    let csd = Csd::from_raw(RawLong::from_registers(words));
    assert_eq!(csd.structure_version(), 2);
    assert_eq!(csd.capacity_bytes(), None);
}
