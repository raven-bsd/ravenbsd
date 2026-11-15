// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The official RP1 firmware record.
//!
//! The blob is **not** part of this crate and must never be linked into the kernel. Before a load
//! the host records four facts: where the file came from, its SHA-256, its size and its licence.
//! The licence and the redistribution decision belong to the owner
//! (`architecture/rpi5/04-firmware-and-provenance.md` section 6).

use core::fmt;

/// Path marker used until the bench records the real firmware path.
pub const UNKNOWN_PATH: &str = "UNKNOWN: record the official RP1 firmware path on the bench";
/// Licence marker used until the owner records the real licence.
pub const UNKNOWN_LICENCE: &str = "UNKNOWN: the RP1 firmware licence is an owner decision";

/// Why a hexadecimal digest is invalid.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DigestError {
    /// The text is not exactly 64 hexadecimal characters.
    Length(usize),
    /// A character is not a hexadecimal digit.
    Digit {
        /// Zero-based character position.
        index: usize,
        /// The offending byte.
        byte: u8,
    },
}

impl fmt::Display for DigestError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Length(length) => write!(f, "SHA-256 digest has {length} characters, not 64"),
            Self::Digit { index, byte } => {
                write!(
                    f,
                    "digest character {index} is not hexadecimal: {byte:#04x}"
                )
            }
        }
    }
}

impl core::error::Error for DigestError {}

/// A 32-byte SHA-256 value.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Digest {
    bytes: [u8; 32],
}

impl Digest {
    /// Wrap raw bytes.
    #[must_use]
    pub const fn from_bytes(bytes: [u8; 32]) -> Self {
        Self { bytes }
    }

    /// Parse the conventional 64-character lowercase or uppercase hexadecimal form.
    ///
    /// # Errors
    ///
    /// Returns [`DigestError`] for a wrong length or a non-hexadecimal character.
    pub fn from_hex(text: &str) -> Result<Self, DigestError> {
        let raw = text.as_bytes();
        if raw.len() != 64 {
            return Err(DigestError::Length(raw.len()));
        }
        let mut bytes = [0u8; 32];
        let mut index = 0usize;
        while index < raw.len() {
            let high = hex_digit(raw, index)?;
            let low = hex_digit(raw, index.saturating_add(1))?;
            let slot = index / 2;
            if let Some(byte) = bytes.get_mut(slot) {
                *byte = (high << 4) | low;
            }
            index = index.saturating_add(2);
        }
        Ok(Self { bytes })
    }

    /// The raw digest bytes.
    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.bytes
    }
}

fn hex_digit(raw: &[u8], index: usize) -> Result<u8, DigestError> {
    let byte = raw
        .get(index)
        .copied()
        .ok_or(DigestError::Length(raw.len()))?;
    match byte {
        b'0'..=b'9' => Ok(byte - b'0'),
        b'a'..=b'f' => Ok(byte - b'a' + 10),
        b'A'..=b'F' => Ok(byte - b'A' + 10),
        _ => Err(DigestError::Digit { index, byte }),
    }
}

impl fmt::Display for Digest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for byte in &self.bytes {
            write!(f, "{byte:02x}")?;
        }
        Ok(())
    }
}

/// What is known about the official firmware image.
///
/// Every field is plain data so the record can live in a static table or a boot log. A record is
/// usable only when [`FirmwareImage::is_recorded`] is true; the state machine refuses to start a
/// transfer otherwise.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FirmwareImage {
    /// Where the file came from (not a path inside this repository).
    pub path: &'static str,
    /// SHA-256 of the file, when it has been computed on the bench.
    pub digest: Option<Digest>,
    /// File length in bytes.
    pub size: Option<u64>,
    /// Licence or distribution terms recorded for the file.
    pub licence: &'static str,
}

impl FirmwareImage {
    /// The empty record: no path, no digest, no size and no licence claim.
    pub const UNRECORDED: Self = Self {
        path: UNKNOWN_PATH,
        digest: None,
        size: None,
        licence: UNKNOWN_LICENCE,
    };

    /// Build a complete record.
    #[must_use]
    pub const fn recorded(
        path: &'static str,
        digest: Digest,
        size: u64,
        licence: &'static str,
    ) -> Self {
        Self {
            path,
            digest: Some(digest),
            size: Some(size),
            licence,
        }
    }

    /// True when the path, digest, size and licence are all present.
    #[must_use]
    pub fn is_recorded(&self) -> bool {
        self.digest.is_some()
            && self.size.is_some()
            && !self.path.is_empty()
            && !self.path.starts_with("UNKNOWN")
            && !self.licence.is_empty()
            && !self.licence.starts_with("UNKNOWN")
    }
}

#[cfg(test)]
#[path = "firmware_ut.rs"]
mod tests;
