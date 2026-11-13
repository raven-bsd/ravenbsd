// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `wcsftime`: UTF-8 bridge to `strftime`.
#![allow(clippy::arithmetic_side_effects)] // RavenBSD: the buffer sizes are bounded by the caller.

use crate::locale::TimeLocale;
use crate::strftime::{StrftimeCtx, strftime};
use crate::tz::{Tm, TzState};

/// Format a wide format string into a wide output buffer.
pub fn wcsftime(
    state: &TzState,
    locale: &dyn TimeLocale,
    out: &mut [u32],
    format: &[u32],
    tm: &Tm,
    ctx: &StrftimeCtx<'_>,
) -> usize {
    let mut utf8 = alloc::vec::Vec::with_capacity(format.len());
    for unit in format {
        let Some(ch) = char::from_u32(*unit) else {
            return 0;
        };
        let mut buffer = [0u8; 4];
        let encoded = ch.encode_utf8(&mut buffer);
        utf8.extend_from_slice(encoded.as_bytes());
    }
    let mut bytes = alloc::vec![0u8; out.len().saturating_mul(4).saturating_add(1)];
    let length = strftime(state, locale, &mut bytes, &utf8, tm, ctx);
    if length == 0 {
        return 0;
    }
    let text = match core::str::from_utf8(bytes.get(..length).unwrap_or(b"")) {
        Ok(text) => text,
        Err(_) => return 0,
    };
    let units: alloc::vec::Vec<u32> = text.chars().map(|ch| ch as u32).collect();
    if units.len() + 1 > out.len() {
        return 0;
    }
    for (slot, unit) in out.iter_mut().zip(units.iter().copied()) {
        *slot = unit;
    }
    if let Some(last) = out.get_mut(units.len()) {
        *last = 0;
    }
    units.len() + 1
}

#[cfg(test)]
#[path = "wcsftime_ut.rs"]
mod tests;
