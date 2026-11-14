// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Target 14: `inet_pton`, `inet_aton`, `inet_ntop` (C51).
//!
//! The body exercises the strict and legacy parsers, the formatter/parser round trip and the
//! small-buffer contract.

use raven_libc_resolv::{inet_aton, inet_ntop, inet_pton};

use crate::{Cursor, alloc_probe};

const AF_INET: i32 = 2;
const AF_INET6: i32 = 10;

fn hash(parts: &[u64]) -> u64 {
    let mut value = 0xcbf2_9ce4_8422_2325u64;
    for part in parts {
        value ^= *part;
        value = value.wrapping_mul(0x100_0000_01b3);
    }
    value
}

/// Run target 14.
pub fn body(data: &[u8]) {
    let mut cursor = Cursor::new(data);
    let selector = cursor.u8() % 3;
    let family = match selector {
        0 => AF_INET,
        1 => AF_INET6,
        _ => 99,
    };
    let text = cursor.take(64);

    let mut address = [0u8; 16];
    let parsed = inet_pton(family, text, &mut address);
    assert!(
        parsed == 1 || parsed == 0 || parsed == -1,
        "P-inet-2: inet_pton returned {parsed}"
    );
    if family != AF_INET && family != AF_INET6 {
        assert!(
            parsed == -1,
            "P-inet-2: an unknown family did not return -1"
        );
    }
    if parsed == 1 {
        let length = if family == AF_INET { 4 } else { 16 };
        let mut rendered = [0u8; 64];
        match inet_ntop(family, address.get(..length).unwrap_or(&[]), &mut rendered) {
            Ok(size) => {
                let bound = if family == AF_INET { 15 } else { 45 };
                assert!(
                    size <= bound,
                    "P-inet-4: a formatted address exceeded its documented maximum"
                );
                let mut again = [0u8; 16];
                let round = inet_pton(family, rendered.get(..size).unwrap_or(&[]), &mut again);
                assert!(
                    round == 1 && again.get(..length) == address.get(..length),
                    "P-inet-3: inet_ntop then inet_pton changed the address"
                );
            }
            Err(_) => {
                let refused = true;
                assert!(!refused, "P-inet-4: a 64-byte buffer was refused");
            }
        }
        for size in [0usize, 7, 15, 16, 46] {
            let mut small = [0xaa_u8; 64];
            let result = inet_ntop(
                family,
                address.get(..length).unwrap_or(&[]),
                small.get_mut(..size).unwrap_or(&mut []),
            );
            if let Ok(written) = result {
                let bound = if family == AF_INET { 15 } else { 45 };
                assert!(
                    written <= bound && written <= size,
                    "P-inet-4: an undersized formatter result was accepted"
                );
            }
        }
    }

    if let Some(address) = inet_aton(text) {
        let mut rendered = [0u8; 16];
        if let Ok(size) = inet_ntop(AF_INET, &address, &mut rendered) {
            let mut again = [0u8; 4];
            assert!(
                inet_pton(AF_INET, rendered.get(..size).unwrap_or(&[]), &mut again) == 1,
                "P-inet-5: the legacy parser produced an unprintable address"
            );
        }
    }

    alloc_probe::note(hash(&[
        u64::from(parsed as u8),
        u64::from(text.len() as u32),
        u64::from(u32::from_le_bytes([
            address[0], address[1], address[2], address[3],
        ])),
    ]));
}

#[cfg(test)]
#[path = "inet_text_ut.rs"]
mod tests;
