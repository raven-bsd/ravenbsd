// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The mailbox envelope, decoded through a caller-supplied layout.
//!
//! **The RP1 mailbox encoding is not public.** A [`Layout`] is data the board bring-up supplies
//! after a bench observation; this crate never guesses one. The decoder validates the envelope
//! shape (magic, version, sequence, length, optional checksum) and is the place where malformed
//! responses are rejected before the state machine sees them.
//!
//! The checksum model is deliberately simple: the 16-bit field in the header must equal the XOR
//! of the header (with the field cleared) and the payload words. A different firmware scheme is a
//! layout change, not a code change.

use core::fmt;

/// Largest payload a model frame accepts.
pub const MAX_FRAME_WORDS: u32 = 8;

/// One mailbox layout, supplied by the bring-up.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Layout {
    /// Header bits that identify the firmware mailbox.
    pub magic: u32,
    /// Mask selecting the magic bits.
    pub magic_mask: u32,
    /// Protocol version the host expects.
    pub version: u32,
    /// Bit position of the version field.
    pub version_shift: u8,
    /// Mask of the version field.
    pub version_mask: u32,
    /// Bit position of the sequence field.
    pub sequence_shift: u8,
    /// Mask of the sequence field; the result must fit 16 bits.
    pub sequence_mask: u32,
    /// Bit position of the payload-length field.
    pub length_shift: u8,
    /// Mask of the payload-length field.
    pub length_mask: u32,
    /// Bit position of the raw status field.
    pub status_shift: u8,
    /// Mask of the raw status field.
    pub status_mask: u32,
    /// True when the message ends with a checksum word.
    pub checksum: bool,
}

/// Why a response cannot be decoded.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FrameError {
    /// The response has no header word.
    Empty,
    /// A layout field is unusable (zero mask, shift above 31, or a field that overflows).
    Layout,
    /// The header magic differs from the layout magic.
    BadMagic {
        /// The header that was read.
        header: u32,
    },
    /// The protocol version differs from the layout version.
    BadVersion {
        /// The version that was read.
        version: u32,
    },
    /// The sequence field does not fit 16 bits.
    BadSequence {
        /// The sequence that was read.
        sequence: u32,
    },
    /// The length field is wider than [`MAX_FRAME_WORDS`].
    LengthTooLarge {
        /// The length that was read.
        length: u32,
    },
    /// The payload is shorter than the length field says.
    Truncated {
        /// The length that was read.
        length: u32,
        /// Words actually present.
        words: usize,
    },
    /// The checksum word differs from the computed value.
    Checksum {
        /// The word from the message.
        expected: u16,
        /// The computed value.
        observed: u16,
    },
}

impl fmt::Display for FrameError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => f.write_str("empty mailbox response"),
            Self::Layout => f.write_str("unusable mailbox layout"),
            Self::BadMagic { header } => write!(f, "bad mailbox magic in {header:#x}"),
            Self::BadVersion { version } => write!(f, "bad mailbox version {version}"),
            Self::BadSequence { sequence } => write!(f, "sequence {sequence} does not fit 16 bits"),
            Self::LengthTooLarge { length } => {
                write!(f, "payload length {length} exceeds {MAX_FRAME_WORDS} words")
            }
            Self::Truncated { length, words } => {
                write!(f, "payload length {length} but only {words} words present")
            }
            Self::Checksum { expected, observed } => {
                write!(f, "checksum {expected:#06x} does not match {observed:#06x}")
            }
        }
    }
}

impl core::error::Error for FrameError {}

/// One decoded response envelope.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Frame {
    /// Sequence number echoed by the firmware.
    pub sequence: u16,
    /// Payload word count.
    pub length: u32,
    /// Raw status field. Its values are firmware-defined and UNVERIFIED.
    pub status: u32,
}

impl Layout {
    /// Validate the shifts and masks.
    ///
    /// # Errors
    ///
    /// Returns [`FrameError::Layout`] for a zero mask, a shift of 32 or more, a field that does
    /// not fit 32 bits or a sequence wider than 16 bits.
    pub fn validate(&self) -> Result<(), FrameError> {
        for (mask, shift) in [
            (self.magic_mask, 0u8),
            (self.version_mask, self.version_shift),
            (self.sequence_mask, self.sequence_shift),
            (self.length_mask, self.length_shift),
            (self.status_mask, self.status_shift),
        ] {
            if mask == 0 || shift >= 32 {
                return Err(FrameError::Layout);
            }
            if (u64::from(mask) << shift) > u64::from(u32::MAX) {
                return Err(FrameError::Layout);
            }
        }
        if self.sequence_mask > 0xFFFF {
            return Err(FrameError::Layout);
        }
        Ok(())
    }
}

fn field(word: u32, shift: u8, mask: u32) -> u32 {
    (word >> shift) & mask
}

/// Decode a response.
///
/// # Errors
///
/// Returns [`FrameError`] for an empty response, an unusable layout, a magic/version mismatch, a
/// sequence wider than 16 bits, an oversized or truncated payload, or a checksum mismatch.
pub fn decode(words: &[u32], layout: &Layout) -> Result<Frame, FrameError> {
    layout.validate()?;
    let header = *words.first().ok_or(FrameError::Empty)?;
    if header & layout.magic_mask != layout.magic & layout.magic_mask {
        return Err(FrameError::BadMagic { header });
    }
    let version = field(header, layout.version_shift, layout.version_mask);
    if version != layout.version {
        return Err(FrameError::BadVersion { version });
    }
    let sequence = field(header, layout.sequence_shift, layout.sequence_mask);
    if sequence > 0xFFFF {
        return Err(FrameError::BadSequence { sequence });
    }
    let length = field(header, layout.length_shift, layout.length_mask);
    if length > MAX_FRAME_WORDS {
        return Err(FrameError::LengthTooLarge { length });
    }
    let needed = if layout.checksum {
        (length as usize).saturating_add(2)
    } else {
        (length as usize).saturating_add(1)
    };
    if needed > words.len() {
        return Err(FrameError::Truncated {
            length,
            words: words.len(),
        });
    }
    if layout.checksum {
        let mut checksum = header;
        for word in words.iter().skip(1).take(length as usize) {
            checksum ^= *word;
        }
        let observed = checksum as u16;
        let expected = words
            .get(1 + length as usize)
            .copied()
            .map_or(0, |word| word as u16);
        if expected != observed {
            return Err(FrameError::Checksum { expected, observed });
        }
    }
    let status = field(header, layout.status_shift, layout.status_mask);
    Ok(Frame {
        sequence: sequence as u16,
        length,
        status,
    })
}

#[cfg(test)]
#[path = "frame_ut.rs"]
mod tests;
