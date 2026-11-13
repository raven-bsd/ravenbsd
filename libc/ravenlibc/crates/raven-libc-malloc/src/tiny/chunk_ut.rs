// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

use super::*;

#[test]
fn chunk_size_adds_the_header_and_aligns() {
    assert_eq!(chunk_size(0), Some(16));
    assert_eq!(chunk_size(1), Some(16));
    assert_eq!(chunk_size(8), Some(16));
    assert_eq!(chunk_size(9), Some(32));
    assert_eq!(chunk_size(24), Some(32));
    assert_eq!(chunk_size(25), Some(48));
}

#[test]
fn chunk_size_rejects_overflow() {
    assert_eq!(chunk_size(usize::MAX), None);
}

#[test]
fn header_and_payload_addresses_round_trip() {
    let chunk = 0x1000usize;
    assert_eq!(size_ref(chunk), chunk - 8);
    assert_eq!(chunk_to_blob(chunk), chunk - 8);
    assert_eq!(blob_to_chunk(chunk_to_blob(chunk)), chunk);
    assert_eq!(chunk_to_ptr(chunk), chunk);
    assert_eq!(ptr_to_chunk(chunk), chunk);
}

#[test]
fn minimum_chunk_has_eight_usable_bytes() {
    assert_eq!(CHUNK_MIN - HEAD_SIZE, 8);
    assert_eq!(CHUNK_MIN, 16);
    assert_eq!(HEAD_SIZE, 8);
}

#[test]
fn chunk_size_rounds_to_sixteen() {
    assert_eq!(chunk_size(7), Some(16));
    assert_eq!(chunk_size(8), Some(16));
    assert_eq!(chunk_size(9), Some(32));
}

#[test]
fn chunk_end_is_size_word_plus_size() {
    let mut buffer = [0u8; 64];
    let chunk = buffer.as_mut_ptr() as usize + 8;
    set_size(chunk, 16);
    assert_eq!(chunk_end(chunk), chunk - 8 + 16);
}

#[test]
fn chunk_after_uses_the_payload_address() {
    let mut buffer = [0u8; 64];
    let chunk = buffer.as_mut_ptr() as usize + 8;
    set_size(chunk, 16);
    assert_eq!(chunk_after(chunk), chunk + 16);
}

#[test]
fn blob_round_trip_is_16_aligned() {
    assert_eq!(blob_to_chunk(0x1000), 0x1010);
    assert_eq!(chunk_to_blob(0x1010), 0x1008);
}

#[test]
fn header_size_matches_the_size_type() {
    assert_eq!(HEAD_SIZE, core::mem::size_of::<usize>());
}
