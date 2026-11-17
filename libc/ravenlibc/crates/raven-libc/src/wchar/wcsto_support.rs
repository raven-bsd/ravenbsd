// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! Shared UTF-8 bridge for the wide numeric conversions.

use alloc::vec::Vec;

pub(crate) fn encode(input: *const u32) -> Vec<u8> {
    let mut out = Vec::new();
    let mut index = 0usize;
    loop {
        // SAFETY: the caller guarantees a NUL-terminated wide string.
        let unit = unsafe { *input.add(index) };
        if unit == 0 {
            break;
        }
        let ch = char::from_u32(unit).unwrap_or('\u{fffd}');
        let mut buffer = [0u8; 4];
        out.extend_from_slice(ch.encode_utf8(&mut buffer).as_bytes());
        index += 1;
    }
    out.push(0);
    out
}

pub(crate) fn units_for_bytes(input: *const u32, bytes: usize) -> usize {
    let mut consumed = 0usize;
    let mut index = 0usize;
    while consumed < bytes {
        // SAFETY: the caller guarantees a NUL-terminated wide string.
        // SAFETY: the caller guarantees a NUL-terminated wide string.
        let unit = unsafe { *input.add(index) };
        let ch = char::from_u32(unit).unwrap_or('\u{fffd}');
        let mut buffer = [0u8; 4];
        let len = ch.encode_utf8(&mut buffer).len();
        if consumed + len > bytes {
            break;
        }
        consumed += len;
        index += 1;
    }
    index
}
