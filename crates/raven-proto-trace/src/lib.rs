// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Pure trace-context wire codec.
//!
//! Q2 (`raven-trace-core`) models the trace id, span tree and sampling; this crate defines the one
//! field that crosses the kernel boundary: a fixed little-endian 64-bit `trace_id` (`R-IPC-4`,
//! `R-OBS-6`). It is a pure codec with no allocation, no kernel dependency and no panic on bad
//! input: a missing id (zero) is reported as [`is_untraced`], a malformed field is a typed
//! [`TraceError`] that the caller drops and counts.
//!
//! # Overhead when disabled
//!
//! The field is always present and fixed-size, so a disabled consumer pays one predicted branch
//! (is the id zero?) and an 8-byte copy — never an allocation and never a variable-length decode.
//! The kernel stamps and propagates the field unchanged; this crate only encodes and decodes it.

#![cfg_attr(not(test), no_std)]
#![forbid(unsafe_code)]

/// The encoded field length: one 64-bit little-endian trace id.
pub const FIELD_LEN: usize = 8;

/// The trace id that means "not traced"; the field is treated as absent.
pub const UNTRACED: u64 = 0;

/// Why a trace-context field could not be decoded.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TraceError {
    /// The input ended before the 8-byte field.
    TooShort,
    /// A strict parse received more than 8 bytes.
    TrailingBytes,
}

impl TraceError {
    /// A stable short name for logs and telemetry.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::TooShort => "too-short",
            Self::TrailingBytes => "trailing-bytes",
        }
    }
}

impl core::fmt::Display for TraceError {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter.write_str(self.as_str())
    }
}

/// Decode a little-endian trace id from the start of `bytes`.
///
/// Returns [`TraceError::TooShort`] when fewer than [`FIELD_LEN`] bytes are present; extra bytes
/// after the field are ignored (the field is one of many in the enclosing header).
pub fn parse(bytes: &[u8]) -> Result<u64, TraceError> {
    let field = bytes.get(..FIELD_LEN).ok_or(TraceError::TooShort)?;
    Ok(decode(field))
}

/// Decode a trace id from exactly [`FIELD_LEN`] bytes; trailing bytes are an error.
pub fn parse_exact(bytes: &[u8]) -> Result<u64, TraceError> {
    if bytes.len() < FIELD_LEN {
        return Err(TraceError::TooShort);
    }
    if bytes.len() > FIELD_LEN {
        return Err(TraceError::TrailingBytes);
    }
    Ok(decode(bytes))
}

/// Encode a trace id into `out`, which must hold at least [`FIELD_LEN`] bytes.
pub fn encode(out: &mut [u8], id: u64) -> Result<(), TraceError> {
    let field = out.get_mut(..FIELD_LEN).ok_or(TraceError::TooShort)?;
    field.copy_from_slice(&id.to_le_bytes());
    Ok(())
}

/// Format a trace id as an 8-byte little-endian array.
pub fn format(id: u64) -> [u8; FIELD_LEN] {
    id.to_le_bytes()
}

/// Whether a decoded id means "not traced" (the field is treated as absent).
pub const fn is_untraced(id: u64) -> bool {
    id == UNTRACED
}

fn decode(bytes: &[u8]) -> u64 {
    let mut array = [0u8; FIELD_LEN];
    array.copy_from_slice(bytes);
    u64::from_le_bytes(array)
}

#[cfg(test)]
#[path = "lib_ut.rs"]
mod tests;
