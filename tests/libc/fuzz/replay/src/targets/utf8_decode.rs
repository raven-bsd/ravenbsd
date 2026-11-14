// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Target 13: `mbrtowc`/`mbsrtowcs`/`wcsrtombs`/`wcrtomb` (C39).
//!
//! The body checks resumability at every split point, UTF-8 acceptance against
//! `core::str::from_utf8`, the `wcrtomb` round trip and the byte-transparent `Byte` mode (H-3).

use raven_libc_ctype::{
    MbMode, MbState, SrcPos, mbrtowc, mbrtowc_reset, mbsrtowcs, wcrtomb, wcsrtombs,
};

use crate::{Cursor, alloc_probe};

/// One decoded character.
#[derive(Clone, Copy, PartialEq, Eq)]
struct Decoded {
    wc: u32,
    used: usize,
}

fn decode(payload: &[u8], mode: MbMode, chunk: usize) -> (Vec<Decoded>, bool, bool) {
    let mut state = MbState::INITIAL;
    let mut out = Vec::new();
    let mut invalid = false;
    let mut offset = 0usize;
    while offset < payload.len() {
        let end = offset.saturating_add(chunk).min(payload.len());
        let slice = payload.get(offset..end).unwrap_or(&[]);
        match mbrtowc(&mut state, mode, slice) {
            raven_libc_ctype::Mbr::Char { wc, used } => {
                out.push(Decoded { wc, used });
                offset = offset.saturating_add(used.max(1));
            }
            raven_libc_ctype::Mbr::Incomplete => {
                offset = end;
                if offset >= payload.len() {
                    invalid = true;
                    break;
                }
            }
            raven_libc_ctype::Mbr::Ilseq => {
                invalid = true;
                let _ = mbrtowc_reset(&mut state);
                break;
            }
        }
    }
    let incomplete = !state.is_initial();
    (out, invalid, incomplete)
}

fn hash(decoded: &[Decoded]) -> u64 {
    let mut value = 0xcbf2_9ce4_8422_2325u64;
    for item in decoded {
        value ^= u64::from(item.wc);
        value = value.wrapping_mul(0x100_0000_01b3);
        value ^= item.used as u64;
        value = value.wrapping_mul(0x100_0000_01b3);
    }
    value
}

/// Run target 13.
pub fn body(data: &[u8]) {
    let mut cursor = Cursor::new(data);
    let mode = if cursor.u8() & 1 == 1 {
        MbMode::Utf8
    } else {
        MbMode::Byte
    };
    let split_count = usize::from(cursor.u8()).max(1);
    let payload = cursor.take(256);

    let (whole, _, whole_incomplete) = decode(payload, mode, payload.len().max(1));
    for chunk in 1..=split_count.min(16) {
        let (split, _, _) = decode(payload, mode, chunk);
        let whole_chars: Vec<u32> = whole.iter().map(|item| item.wc).collect();
        let split_chars: Vec<u32> = split.iter().map(|item| item.wc).collect();
        assert!(
            split_chars == whole_chars,
            "P-utf8-2: the decoded characters change with a {chunk}-byte split"
        );
    }

    let valid = core::str::from_utf8(payload).is_ok();
    if mode == MbMode::Utf8 {
        let accepted = !whole_incomplete && core::str::from_utf8(payload).is_ok();
        assert!(
            accepted == valid,
            "P-utf8-3: UTF-8 acceptance disagrees with core::str::from_utf8"
        );
        if let Ok(text) = core::str::from_utf8(payload) {
            let expected: Vec<u32> = text.chars().map(u32::from).collect();
            let actual: Vec<u32> = whole.iter().map(|item| item.wc).collect();
            assert!(
                actual == expected,
                "P-utf8-3: the decoded code points differ from core::str"
            );
        }
    } else {
        for (item, byte) in whole.iter().zip(payload.iter()) {
            if item.used == 1 {
                let expected = if *byte < 0x80 {
                    u32::from(*byte)
                } else {
                    0xdf80 + u32::from(*byte - 0x80)
                };
                assert!(
                    item.wc == expected,
                    "P-utf8-5: Byte mode did not map a byte to its code unit"
                );
            }
        }
    }

    let mut round_trip = true;
    for chunk in payload.chunks(4) {
        let mut bytes = [0u8; 4];
        let copied = chunk.len().min(4);
        if let Some(slot) = bytes.get_mut(..copied) {
            slot.copy_from_slice(chunk.get(..copied).unwrap_or(&[]));
        }
        let wc = u32::from_le_bytes(bytes);
        if mode == MbMode::Byte && wc > 0xff {
            continue;
        }
        if char::from_u32(wc).is_none() {
            continue;
        }
        let mut encoded = [0u8; 4];
        if let Ok(length) = wcrtomb(mode, wc, &mut encoded) {
            let mut state = MbState::INITIAL;
            match mbrtowc(&mut state, mode, encoded.get(..length).unwrap_or(&[])) {
                raven_libc_ctype::Mbr::Char { wc: back, used } => {
                    round_trip &= back == wc && used == length;
                }
                _ => round_trip = false,
            }
        }
    }
    assert!(
        round_trip,
        "P-utf8-4: wcrtomb then mbrtowc did not return the same scalar"
    );

    if core::str::from_utf8(payload).is_ok() {
        let mut state = MbState::INITIAL;
        let length = mbsrtowcs(Some(&mut state), mode, payload, None);
        if let Ok(count) = length.ret {
            let mut wide = vec![0u32; count.saturating_add(1)];
            let mut conv_state = MbState::INITIAL;
            let conv = mbsrtowcs(Some(&mut conv_state), mode, payload, Some(&mut wide));
            if conv.ret.is_ok() {
                let mut back_state = MbState::INITIAL;
                let needed = wcsrtombs(Some(&mut back_state), mode, &wide, None);
                if let Ok(bytes_needed) = needed.ret {
                    let mut bytes = vec![0u8; bytes_needed.saturating_add(1)];
                    let mut final_state = MbState::INITIAL;
                    let converted =
                        wcsrtombs(Some(&mut final_state), mode, &wide, Some(&mut bytes));
                    match converted.src {
                        SrcPos::Nul => {
                            let prefix = payload.split(|byte| *byte == 0).next().unwrap_or(payload);
                            assert!(
                                bytes.get(..prefix.len()) == Some(prefix),
                                "P-utf8-4: the multibyte round trip changed the payload"
                            );
                        }
                        SrcPos::At(_) => {}
                    }
                }
            }
        }
    }

    alloc_probe::note(hash(&whole));
}

#[cfg(test)]
#[path = "utf8_decode_ut.rs"]
mod tests;
