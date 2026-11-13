// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Fixed-corpus loading and byte-string parsing.
//!
//! A corpus file is `corpus/<function>/<name>.toml` with one or more `[[case]]` tables whose keys
//! are the case struct's fields. Byte strings are written as `bytes = "text"` (ASCII, no NUL) or
//! `bytes_hex = "00ff..."`. Unknown keys and an empty file are errors: a silently empty corpus is
//! the failure mode this format is designed to prevent.

use std::fs;
use std::path::Path;

use serde::de::DeserializeOwned;

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct CorpusFile<C> {
    case: Vec<C>,
}

/// Load one corpus file relative to the crate's `corpus` directory.
///
/// # Panics
///
/// Panics when the path is unsafe, the file is missing or malformed, or the file has no cases.
#[must_use]
pub fn load<C: DeserializeOwned>(relative: &str) -> Vec<C> {
    let base = Path::new(env!("CARGO_MANIFEST_DIR")).join("corpus");
    load_from(&base, relative)
}

/// Load one corpus file relative to `base`.
///
/// # Panics
///
/// Panics when the path is unsafe, the file is missing or malformed, or the file has no cases.
#[must_use]
pub(crate) fn load_from<C: DeserializeOwned>(base: &Path, relative: &str) -> Vec<C> {
    let relative_path = Path::new(relative);
    if relative_path.is_absolute()
        || relative_path
            .components()
            .any(|component| !matches!(component, std::path::Component::Normal(_)))
    {
        panic!("corpus path `{relative}` must be relative and without `..`");
    }
    let path = base.join(relative_path);
    let text = fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("cannot read corpus {}: {error}", path.display()));
    let parsed: CorpusFile<C> = toml::from_str(&text)
        .unwrap_or_else(|error| panic!("corpus {} is not valid: {error}", path.display()));
    if parsed.case.is_empty() {
        panic!("corpus {} has no [[case]] entries", path.display());
    }
    parsed.case
}

/// Parse a corpus byte string from the `bytes`/`bytes_hex` pair.
pub(crate) fn parse_bytes(
    bytes: Option<String>,
    hex: Option<String>,
    what: &str,
) -> Result<Vec<u8>, String> {
    match (bytes, hex) {
        (Some(text), None) => {
            if text.bytes().any(|byte| byte == 0 || byte >= 0x80) {
                return Err(format!(
                    "{what}: `bytes` must be ASCII without NUL; use `bytes_hex`"
                ));
            }
            Ok(text.into_bytes())
        }
        (None, Some(hex)) => decode_hex(&hex, what),
        (Some(_), Some(_)) => Err(format!("{what}: give `bytes` or `bytes_hex`, not both")),
        (None, None) => Err(format!("{what}: `bytes` or `bytes_hex` is required")),
    }
}

/// Encode bytes for a `bytes_hex` corpus line.
pub(crate) fn render_bytes(bytes: &[u8]) -> String {
    format!("bytes_hex = \"{}\"", hex_encode(bytes))
}

/// Hex encoding used by the reproducer.
pub(crate) fn hex_encode(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut text = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        text.push(char::from(DIGITS[usize::from(byte >> 4)]));
        text.push(char::from(DIGITS[usize::from(byte & 0x0f)]));
    }
    text
}

fn decode_hex(text: &str, what: &str) -> Result<Vec<u8>, String> {
    if !text.len().is_multiple_of(2) {
        return Err(format!("{what}: `bytes_hex` has an odd number of digits"));
    }
    let mut bytes = Vec::with_capacity(text.len() / 2);
    let raw = text.as_bytes();
    let mut index = 0;
    while index < raw.len() {
        let high = hex_digit(raw[index]).ok_or_else(|| format!("{what}: invalid hex"))?;
        let low = hex_digit(raw[index + 1]).ok_or_else(|| format!("{what}: invalid hex"))?;
        bytes.push((high << 4) | low);
        index += 2;
    }
    Ok(bytes)
}

fn hex_digit(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

#[cfg(test)]
#[path = "corpus_ut.rs"]
mod tests;
