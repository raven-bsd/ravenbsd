// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! W3C `traceparent` (version 00) text codec.
//!
//! The binary [`crate`] codec carries a trace id inside the native IPC header; this module parses
//! and formats the W3C Trace Context `traceparent` form used for external propagation. The grammar
//! is strictly lowercase per the W3C specification:
//!
//! ```text
//! traceparent = "00" "-" trace-id "-" parent-id "-" trace-flags
//! trace-id    = 32HEXDIGLC   ; 16 bytes, not all zero
//! parent-id   = 16HEXDIGLC   ; 8 bytes, not all zero
//! trace-flags = 2HEXDIGLC    ; bit 0 = sampled, others reserved/ignored
//! ```
//!
//! Malformed input is rejected with a typed [`TraceparentError`] — never a panic and never an
//! allocation.

/// Exact encoded length: `00-` + 32 + `-` + 16 + `-` + 2.
pub const LEN: usize = 55;
/// The supported version.
pub const VERSION: u8 = 0;
/// The sampled flag bit in `trace-flags`.
pub const SAMPLED: u8 = 0x01;

/// A parsed W3C `traceparent`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Traceparent {
    /// 16-byte trace identifier.
    pub trace_id: [u8; 16],
    /// 8-byte parent identifier.
    pub parent_id: [u8; 8],
    /// Trace flags; bit 0 is the sampled flag, other bits are reserved.
    pub flags: u8,
}

impl Traceparent {
    /// Whether the sampled flag is set.
    pub const fn sampled(self) -> bool {
        self.flags & SAMPLED != 0
    }
}

/// Why a `traceparent` could not be parsed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TraceparentError {
    /// The input length is not exactly [`LEN`].
    BadLength,
    /// A field separator is missing or misplaced.
    BadSeparator,
    /// A field contains a non-hex character (uppercase is rejected per the lowercase grammar).
    NonHex,
    /// The version is not `00`.
    UnsupportedVersion,
    /// The trace id is all zero.
    ZeroTraceId,
    /// The parent id is all zero.
    ZeroParentId,
}

impl TraceparentError {
    /// A stable short name for logs and telemetry.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::BadLength => "bad-length",
            Self::BadSeparator => "bad-separator",
            Self::NonHex => "non-hex",
            Self::UnsupportedVersion => "unsupported-version",
            Self::ZeroTraceId => "zero-trace-id",
            Self::ZeroParentId => "zero-parent-id",
        }
    }
}

impl core::fmt::Display for TraceparentError {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter.write_str(self.as_str())
    }
}

/// Parse a W3C `traceparent` (version 00).
pub fn parse(bytes: &[u8]) -> Result<Traceparent, TraceparentError> {
    if bytes.len() != LEN {
        return Err(TraceparentError::BadLength);
    }
    if bytes[2] != b'-' || bytes[35] != b'-' || bytes[52] != b'-' {
        return Err(TraceparentError::BadSeparator);
    }
    let version = hex_byte(bytes[0], bytes[1]).ok_or(TraceparentError::NonHex)?;
    if version != VERSION {
        return Err(TraceparentError::UnsupportedVersion);
    }

    let mut trace_id = [0u8; 16];
    for (index, byte) in trace_id.iter_mut().enumerate() {
        let hi = bytes[3 + index * 2];
        let lo = bytes[3 + index * 2 + 1];
        *byte = hex_byte(hi, lo).ok_or(TraceparentError::NonHex)?;
    }
    if trace_id.iter().all(|&b| b == 0) {
        return Err(TraceparentError::ZeroTraceId);
    }

    let mut parent_id = [0u8; 8];
    for (index, byte) in parent_id.iter_mut().enumerate() {
        let hi = bytes[36 + index * 2];
        let lo = bytes[36 + index * 2 + 1];
        *byte = hex_byte(hi, lo).ok_or(TraceparentError::NonHex)?;
    }
    if parent_id.iter().all(|&b| b == 0) {
        return Err(TraceparentError::ZeroParentId);
    }

    let flags = hex_byte(bytes[53], bytes[54]).ok_or(TraceparentError::NonHex)?;
    Ok(Traceparent {
        trace_id,
        parent_id,
        flags,
    })
}

/// Format a `traceparent` as a 55-byte lowercase string.
pub fn format(traceparent: &Traceparent) -> [u8; LEN] {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = [0u8; LEN];
    out[0] = b'0';
    out[1] = b'0';
    out[2] = b'-';
    for (index, byte) in traceparent.trace_id.iter().enumerate() {
        out[3 + index * 2] = HEX[(byte >> 4) as usize];
        out[3 + index * 2 + 1] = HEX[(byte & 0x0F) as usize];
    }
    out[35] = b'-';
    for (index, byte) in traceparent.parent_id.iter().enumerate() {
        out[36 + index * 2] = HEX[(byte >> 4) as usize];
        out[36 + index * 2 + 1] = HEX[(byte & 0x0F) as usize];
    }
    out[52] = b'-';
    out[53] = HEX[(traceparent.flags >> 4) as usize];
    out[54] = HEX[(traceparent.flags & 0x0F) as usize];
    out
}

fn hex_nibble(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        _ => None,
    }
}

fn hex_byte(hi: u8, lo: u8) -> Option<u8> {
    Some((hex_nibble(hi)? << 4) | hex_nibble(lo)?)
}

#[cfg(test)]
#[path = "traceparent_ut.rs"]
mod tests;
