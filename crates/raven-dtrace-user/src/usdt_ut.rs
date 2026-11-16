// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Unit tests for the USDT marker and note parser.

use super::*;

const ADDRESS: u64 = 0x40_1000;
const DESC_LEN: usize = 24 + 6 + 10 + 9;

fn put_u32(data: &mut [u8], offset: usize, value: u32) {
    if let Some(slot) = data.get_mut(offset..offset.saturating_add(4)) {
        slot.copy_from_slice(&value.to_le_bytes());
    }
}

fn put_u64(data: &mut [u8], offset: usize, value: u64) {
    if let Some(slot) = data.get_mut(offset..offset.saturating_add(8)) {
        slot.copy_from_slice(&value.to_le_bytes());
    }
}

fn put_str(data: &mut [u8], offset: usize, text: &str) {
    if let Some(slot) = data.get_mut(offset..offset.saturating_add(text.len())) {
        slot.copy_from_slice(text.as_bytes());
    }
}

fn note() -> [u8; 128] {
    let mut data = [0u8; 128];
    put_u32(&mut data, 0, 8); // namesz, "stapsdt\0"
    put_u32(&mut data, 4, DESC_LEN as u32);
    put_u32(&mut data, 8, 3); // NT_STAPSDT
    put_str(&mut data, 12, "stapsdt\0");
    put_u64(&mut data, 20, ADDRESS);
    put_u64(&mut data, 28, 0);
    put_u64(&mut data, 36, 0);
    put_str(&mut data, 44, "raven\0usdt-test\0arg0 int\0");
    data
}

#[test]
fn the_marker_reads_as_a_nop() {
    assert_eq!(site_address() % 4, 0);
    assert_eq!(site_opcode(), nop_opcode());
    assert_eq!(nop_opcode(), 0x90);
}

#[test]
fn the_static_probe_describes_the_marker() {
    assert_eq!(USDT_PROBES.len(), 1);
    let probe = &USDT_PROBES[0];
    assert_eq!(probe.provider, "raven");
    assert_eq!(probe.name, "usdt-test");
    assert_eq!(probe.args, &["arg0"]);
    assert_eq!(probe.site_address(), site_address());
}

#[test]
fn a_valid_note_is_decoded() {
    let data = note();
    let decoded = parse_note(&data).expect("note");
    assert_eq!(decoded.address, ADDRESS);
    assert_eq!(decoded.base, 0);
    assert_eq!(decoded.semaphore, 0);
    assert_eq!(decoded.provider, "raven");
    assert_eq!(decoded.name, "usdt-test");
    assert_eq!(decoded.args, "arg0 int");
}

#[test]
fn a_truncated_note_is_rejected() {
    assert_eq!(parse_note(&[]), Err(NoteError::TooShort));
    assert_eq!(parse_note(&[0u8; 8]), Err(NoteError::TooShort));
    let data = note();
    assert_eq!(
        parse_note(data.get(..30).expect("prefix")),
        Err(NoteError::TooShort)
    );
}

#[test]
fn a_bad_name_is_rejected() {
    let mut data = note();
    data[12] = b'x';
    assert_eq!(parse_note(&data), Err(NoteError::BadName));
}

#[test]
fn a_bad_type_is_rejected() {
    let mut data = note();
    put_u32(&mut data, 8, 4);
    assert_eq!(parse_note(&data), Err(NoteError::BadType));
}

#[test]
fn a_missing_nul_terminator_is_rejected() {
    let mut data = note();
    data[68] = b'x';
    assert_eq!(parse_note(&data), Err(NoteError::BadString));
}
