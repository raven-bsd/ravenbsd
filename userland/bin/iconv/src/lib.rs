// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Bounded character-set conversion shared by the `iconv` CLI and tests.
//!
//! The supported sets are UTF-8, ASCII and ISO-8859-1. Conversion is whole-input and fails
//! without producing partial output when an input sequence is invalid or a character cannot be
//! represented in the target set.

#![cfg_attr(target_os = "none", no_std)]
#![forbid(unsafe_code)]
#![warn(missing_docs)]

extern crate alloc;

use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;

/// One supported character set.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Encoding {
    /// UTF-8.
    Utf8,
    /// Seven-bit ASCII.
    Ascii,
    /// ISO-8859-1 (Latin-1).
    Latin1,
}

impl Encoding {
    /// Human-readable name used in diagnostics.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Utf8 => "UTF-8",
            Self::Ascii => "ASCII",
            Self::Latin1 => "ISO-8859-1",
        }
    }
}

/// A conversion failure.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConvertError {
    /// Byte offset in the source data, or character index for an encode failure.
    pub offset: usize,
    /// Diagnostic text.
    pub message: String,
}

/// Resolve a supported encoding name, case-insensitively.
#[must_use]
pub fn parse_encoding(name: &str) -> Option<Encoding> {
    let name = name.trim().to_ascii_lowercase();
    match name.as_str() {
        "utf-8" | "utf8" => Some(Encoding::Utf8),
        "ascii" | "us-ascii" | "ansi_x3.4-1968" => Some(Encoding::Ascii),
        "iso-8859-1" | "iso8859-1" | "latin1" | "latin-1" => Some(Encoding::Latin1),
        _ => None,
    }
}

fn decode(data: &[u8], encoding: Encoding) -> Result<Vec<char>, ConvertError> {
    let mut characters = Vec::new();
    let mut index = 0;
    while index < data.len() {
        let byte = data[index];
        match encoding {
            Encoding::Ascii => {
                if byte > 0x7f {
                    return Err(ConvertError {
                        offset: index,
                        message: "invalid ASCII byte".to_string(),
                    });
                }
                characters.push(char::from(byte));
                index += 1;
            }
            Encoding::Latin1 => {
                characters.push(char::from(byte));
                index += 1;
            }
            Encoding::Utf8 => {
                let Some(character) = decode_utf8(data, index)? else {
                    break;
                };
                characters.push(character);
                index += utf8_len(byte);
            }
        }
    }
    Ok(characters)
}

fn utf8_len(first: u8) -> usize {
    if first < 0x80 {
        1
    } else if first & 0xe0 == 0xc0 {
        2
    } else if first & 0xf0 == 0xe0 {
        3
    } else {
        4
    }
}

fn decode_utf8(data: &[u8], index: usize) -> Result<Option<char>, ConvertError> {
    let first = data[index];
    if first < 0x80 {
        return Ok(Some(char::from(first)));
    }
    let length = if first & 0xe0 == 0xc0 {
        2
    } else if first & 0xf0 == 0xe0 {
        3
    } else if first & 0xf8 == 0xf0 {
        4
    } else {
        return Err(ConvertError {
            offset: index,
            message: "invalid UTF-8 lead byte".to_string(),
        });
    };
    let end = index.saturating_add(length);
    let Some(slice) = data.get(index..end) else {
        return Err(ConvertError {
            offset: index,
            message: "truncated UTF-8 sequence".to_string(),
        });
    };
    let mut value = u32::from(first & (0x7f >> length));
    for byte in &slice[1..] {
        if byte & 0xc0 != 0x80 {
            return Err(ConvertError {
                offset: index,
                message: "invalid UTF-8 continuation byte".to_string(),
            });
        }
        value = (value << 6) | u32::from(byte & 0x3f);
    }
    let minimum = match length {
        2 => 0x80,
        3 => 0x800,
        _ => 0x1_0000,
    };
    if value < minimum || (0xd800..=0xdfff).contains(&value) || value > 0x10_ffff {
        return Err(ConvertError {
            offset: index,
            message: "invalid UTF-8 code point".to_string(),
        });
    }
    let character = char::from_u32(value).ok_or_else(|| ConvertError {
        offset: index,
        message: "invalid UTF-8 code point".to_string(),
    })?;
    Ok(Some(character))
}

fn encode(characters: &[char], encoding: Encoding) -> Result<Vec<u8>, ConvertError> {
    let mut output = Vec::new();
    let mut buffer = [0u8; 4];
    for (index, character) in characters.iter().enumerate() {
        match encoding {
            Encoding::Utf8 => {
                output.extend_from_slice(character.encode_utf8(&mut buffer).as_bytes())
            }
            Encoding::Ascii => {
                let value = *character as u32;
                if value > 0x7f {
                    return Err(ConvertError {
                        offset: index,
                        message: format!("cannot convert U+{value:04X} to ASCII"),
                    });
                }
                output.push(value as u8);
            }
            Encoding::Latin1 => {
                let value = *character as u32;
                if value > 0xff {
                    return Err(ConvertError {
                        offset: index,
                        message: format!("cannot convert U+{value:04X} to ISO-8859-1"),
                    });
                }
                output.push(value as u8);
            }
        }
    }
    Ok(output)
}

/// Convert a complete byte string from one supported encoding to another.
pub fn convert(data: &[u8], from: Encoding, to: Encoding) -> Result<Vec<u8>, ConvertError> {
    let characters = decode(data, from)?;
    encode(&characters, to)
}

#[cfg(test)]
#[path = "lib_ut.rs"]
mod tests;
