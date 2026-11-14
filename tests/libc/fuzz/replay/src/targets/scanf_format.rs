// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Target 2: `scanf`/`wscanf` (C43).
//!
//! The body scans arbitrary formats over a one-slot pushback source and checks the consumed,
//! assignment, token and store-log contracts.

use raven_libc_fmt::{
    C_NUMERIC_FORMAT, FmtErr, NoScanFloat, SCAN_TOKEN_MAX, ScanOptions, ScanSource, vscan,
};

use crate::env::{FuzzEnv, Store};
use crate::{Cursor, alloc_probe};

struct ByteSource<'a> {
    data: &'a [u8],
    pos: usize,
    pushback: Option<u8>,
}

impl ScanSource<u8> for ByteSource<'_> {
    fn getc(&mut self) -> Option<u8> {
        if let Some(byte) = self.pushback.take() {
            return Some(byte);
        }
        let byte = self.data.get(self.pos).copied();
        if byte.is_some() {
            self.pos = self.pos.saturating_add(1);
        }
        byte
    }

    fn ungetc(&mut self, c: u8) {
        self.pushback = Some(c);
    }
}

struct WideSource<'a> {
    data: &'a [u8],
    pos: usize,
    pushback: Option<u32>,
}

impl ScanSource<u32> for WideSource<'_> {
    fn getc(&mut self) -> Option<u32> {
        if let Some(unit) = self.pushback.take() {
            return Some(unit);
        }
        let byte = self.data.get(self.pos).copied();
        if byte.is_some() {
            self.pos = self.pos.saturating_add(1);
        }
        byte.map(u32::from)
    }

    fn ungetc(&mut self, c: u32) {
        self.pushback = Some(c);
    }
}

fn hash(parts: &[u64]) -> u64 {
    let mut value = 0xcbf2_9ce4_8422_2325u64;
    for part in parts {
        value ^= *part;
        value = value.wrapping_mul(0x100_0000_01b3);
    }
    value
}

/// Run target 2.
pub fn body(data: &[u8]) {
    let mut cursor = Cursor::new(data);
    let flags = cursor.u8();
    let wide = flags & 1 != 0;
    let mut env = FuzzEnv::from_cursor(&mut cursor);
    let rest = cursor.rest();
    let split = rest
        .iter()
        .position(|byte| *byte == 0)
        .unwrap_or(rest.len().min(128))
        .min(128);
    let format = rest.get(..split).unwrap_or(&[]);
    let input = rest.get(split.saturating_add(1)..).unwrap_or(&[]);
    let input = input.get(..1024).unwrap_or(input);
    let options = ScanOptions {
        numeric: C_NUMERIC_FORMAT,
        mode: raven_libc_ctype::MbMode::Byte,
    };

    let (result, consumed) = if wide {
        let wide_format: Vec<u32> = format.iter().map(|byte| u32::from(*byte)).collect();
        let mut source = WideSource {
            data: input,
            pos: 0,
            pushback: None,
        };
        let result = vscan(&mut source, &wide_format, &mut env, &options, &NoScanFloat);
        let consumed = source
            .pos
            .saturating_sub(usize::from(source.pushback.is_some()));
        (result, consumed)
    } else {
        let mut source = ByteSource {
            data: input,
            pos: 0,
            pushback: None,
        };
        let result = vscan(&mut source, format, &mut env, &options, &NoScanFloat);
        let consumed = source
            .pos
            .saturating_sub(usize::from(source.pushback.is_some()));
        (result, consumed)
    };

    if let Ok(scan) = result {
        assert!(
            consumed <= input.len(),
            "P-scanf-2: the scanner consumed past the input"
        );
        assert!(
            scan.consumed <= input.len(),
            "P-scanf-2: ScanResult::consumed exceeds the input"
        );
        assert!(scan.assigned >= 0, "P-scanf-3: a negative assignment count");
        if scan.return_eof {
            assert_eq!(scan.assigned, 0, "P-scanf-3: return_eof with assignments");
        }
        assert!(
            env.stores().len() <= 64 && env.dropped_stores() == 0,
            "P-scanf-5: the store log overflowed"
        );
        for store in env.stores() {
            match store {
                Store::Chars { len, .. } | Store::Wide { len, .. } => assert!(
                    *len <= SCAN_TOKEN_MAX.saturating_add(1),
                    "P-scanf-4: a string store exceeded SCAN_TOKEN_MAX"
                ),
                _ => {}
            }
        }
    } else {
        assert!(
            matches!(
                result,
                Err(FmtErr::NoMem) | Err(FmtErr::Encoding) | Err(FmtErr::Invalid)
            ),
            "P-scanf-1: an unexpected scanner error"
        );
    }

    alloc_probe::note(hash(&[
        u64::from(flags),
        consumed as u64,
        env.stores().len() as u64,
        result.map(|scan| scan.assigned as u64).unwrap_or(0),
    ]));
}

#[cfg(test)]
#[path = "scanf_format_ut.rs"]
mod tests;
