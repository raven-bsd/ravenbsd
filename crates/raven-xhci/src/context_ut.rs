// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Assertions for the xHCI input-context encoding.

use super::*;
use crate::error::XhciError;

fn u32_at(bytes: &[u8], offset: usize) -> u32 {
    let mut word = [0u8; 4];
    word.copy_from_slice(&bytes[offset..offset + 4]);
    u32::from_le_bytes(word)
}

fn u64_at(bytes: &[u8], offset: usize) -> u64 {
    let mut word = [0u8; 8];
    word.copy_from_slice(&bytes[offset..offset + 8]);
    u64::from_le_bytes(word)
}

#[test]
fn the_context_sizes_follow_csz() {
    assert_eq!(context_size(false), 32);
    assert_eq!(context_size(true), 64);
    assert_eq!(input_context_size(32), 32 + 32 * 32);
    assert_eq!(input_context_size(64), MAX_INPUT_CONTEXT);
}

#[test]
fn address_device_encodes_slot_and_ep0() {
    let mut bytes = [0u8; MAX_INPUT_CONTEXT];
    encode_address_input(64, 2, 3, 0x8000, 8, &mut bytes).expect("address");
    assert_eq!(u32_at(&bytes, 4), 0x3);
    assert_eq!(u32_at(&bytes, 32), (1 << 27) | (3 << 20));
    assert_eq!(u32_at(&bytes, 36), 2 << 16);
    let ep0 = 32 + 64;
    assert_eq!(u32_at(&bytes, ep0 + 4), (3 << 1) | (4 << 3) | (8 << 16));
    assert_eq!(u64_at(&bytes, ep0 + 8), 0x8001);
    assert_eq!(u32_at(&bytes, ep0 + 16), 8);
}

#[test]
fn configure_endpoint_encodes_ep1_and_keeps_ep0() {
    let mut bytes = [0u8; MAX_INPUT_CONTEXT];
    encode_configure_input(64, 1, 1, 0x8000, 0x9000, 8, 10, &mut bytes).expect("configure");
    assert_eq!(u32_at(&bytes, 4), 0x9);
    assert_eq!(u32_at(&bytes, 32), (3 << 27) | (1 << 20));
    assert_eq!(u64_at(&bytes, 32 + 64 + 8), 0x8001);
    let ep1 = 32 + 3 * 64;
    assert_eq!(u32_at(&bytes, ep1), 10 << 16);
    assert_eq!(u32_at(&bytes, ep1 + 4), (3 << 1) | (3 << 3) | (8 << 16));
    assert_eq!(u64_at(&bytes, ep1 + 8), 0x9001);
    assert_eq!(u32_at(&bytes, ep1 + 16), 8);
}

#[test]
fn invalid_context_sizes_and_short_buffers_are_rejected() {
    let mut bytes = [0u8; MAX_INPUT_CONTEXT];
    assert_eq!(
        encode_address_input(16, 1, 1, 0x8000, 8, &mut bytes),
        Err(XhciError::Length)
    );
    assert_eq!(
        encode_address_input(32, 1, 1, 0x8000, 8, &mut bytes[..64]),
        Err(XhciError::Length)
    );
    assert_eq!(
        encode_configure_input(32, 1, 1, 0x8000, 0x9000, 8, 10, &mut []),
        Err(XhciError::Length)
    );
}
