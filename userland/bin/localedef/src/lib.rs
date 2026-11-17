// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Bounded POSIX `localedef`.
//!
//! The tool parses a locale definition source and a charmap, then emits the runtime catalog
//! image read by `raven_libc_locale::nls::Catalog` (the same `ff88ff89` image `gencat` writes).
//! Locale categories become sets `1..6`; the `LOCALE` set `7` carries the name, charmap and
//! charmap entry count. Unsupported syntax is an error and no image is produced.

#![cfg_attr(not(test), no_std)]
#![forbid(unsafe_code)]
#![warn(missing_docs)]

extern crate alloc;

use alloc::collections::BTreeMap;
use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;

use gencat::{Sets, write_catalog};

/// The utility name.
pub const NAME: &str = "localedef";
/// Usage line.
pub const USAGE: &str = "localedef -i input -f charmap [-c] [-o output] [name]";

/// `LC_CTYPE` catalog set.
pub const LC_CTYPE: i32 = 1;
/// `LC_NUMERIC` catalog set.
pub const LC_NUMERIC: i32 = 2;
/// `LC_TIME` catalog set.
pub const LC_TIME: i32 = 3;
/// `LC_COLLATE` catalog set.
pub const LC_COLLATE: i32 = 4;
/// `LC_MONETARY` catalog set.
pub const LC_MONETARY: i32 = 5;
/// `LC_MESSAGES` catalog set.
pub const LC_MESSAGES: i32 = 6;
/// The locale metadata set.
pub const LOCALE: i32 = 7;
/// The message id used for `copy "..."`.
const COPY_ID: i32 = 100;
/// The `LOCALE` message ids.
const LOCALE_NAME_ID: i32 = 1;
const LOCALE_CHARMAP_ID: i32 = 2;
const LOCALE_CHARMAP_ENTRIES_ID: i32 = 3;
const LOCALE_CATEGORIES_ID: i32 = 4;

/// Why a `localedef` invocation failed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LocaledefError {
    /// The command line is invalid.
    Usage,
    /// The source syntax is outside the bounded subset.
    Unsupported(String),
    /// The source is malformed.
    Malformed(String),
}

impl core::fmt::Display for LocaledefError {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Usage => formatter.write_str("invalid usage"),
            Self::Unsupported(value) => write!(formatter, "unsupported: {value}"),
            Self::Malformed(value) => write!(formatter, "malformed: {value}"),
        }
    }
}

/// Parsed command line.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Options {
    /// `-i` locale definition source.
    pub input: String,
    /// `-f` charmap source.
    pub charmap: String,
    /// `-o` output path; stdout when absent.
    pub output: Option<String>,
    /// Optional locale name operand.
    pub name: Option<String>,
}

/// Parse the documented option set.
pub fn parse_args(arguments: &[String]) -> Result<Options, LocaledefError> {
    let mut input = None;
    let mut charmap = None;
    let mut output = None;
    let mut name = None;
    let mut index = 0usize;
    while index < arguments.len() {
        let argument = match arguments.get(index) {
            Some(value) => value,
            None => break,
        };
        match argument.as_str() {
            "--" => {
                index = index.saturating_add(1);
                if let Some(value) = arguments.get(index) {
                    if name.is_some() {
                        return Err(LocaledefError::Usage);
                    }
                    name = Some(value.clone());
                    index = index.saturating_add(1);
                }
                if index < arguments.len() {
                    return Err(LocaledefError::Usage);
                }
                break;
            }
            "-c" => {}
            "-i" | "-f" | "-o" => {
                index = index.saturating_add(1);
                let Some(value) = arguments.get(index) else {
                    return Err(LocaledefError::Usage);
                };
                match argument.as_str() {
                    "-i" => input = Some(value.clone()),
                    "-f" => charmap = Some(value.clone()),
                    _ => output = Some(value.clone()),
                }
            }
            value
                if value.starts_with("-i")
                    || value.starts_with("-f")
                    || value.starts_with("-o") =>
            {
                let (flag, attached) = value.split_at(2);
                if attached.is_empty() {
                    return Err(LocaledefError::Usage);
                }
                match flag {
                    "-i" => input = Some(attached.to_string()),
                    "-f" => charmap = Some(attached.to_string()),
                    _ => output = Some(attached.to_string()),
                }
            }
            value if value.starts_with('-') => return Err(LocaledefError::Usage),
            value => {
                if name.is_some() {
                    return Err(LocaledefError::Usage);
                }
                name = Some(value.to_string());
            }
        }
        index = index.saturating_add(1);
    }
    let (Some(input), Some(charmap)) = (input, charmap) else {
        return Err(LocaledefError::Usage);
    };
    Ok(Options {
        input,
        charmap,
        output,
        name,
    })
}

fn category(line: &str) -> Option<i32> {
    match line {
        "LC_CTYPE" => Some(LC_CTYPE),
        "LC_NUMERIC" => Some(LC_NUMERIC),
        "LC_TIME" => Some(LC_TIME),
        "LC_COLLATE" => Some(LC_COLLATE),
        "LC_MONETARY" => Some(LC_MONETARY),
        "LC_MESSAGES" => Some(LC_MESSAGES),
        _ => None,
    }
}

fn category_name(set: i32) -> &'static str {
    match set {
        LC_CTYPE => "LC_CTYPE",
        LC_NUMERIC => "LC_NUMERIC",
        LC_TIME => "LC_TIME",
        LC_COLLATE => "LC_COLLATE",
        LC_MONETARY => "LC_MONETARY",
        LC_MESSAGES => "LC_MESSAGES",
        _ => "LOCALE",
    }
}

fn keyword_id(set: i32, keyword: &str) -> Option<i32> {
    let table: &[&str] = match set {
        LC_CTYPE => &["codeset", "mb_cur_max"],
        LC_NUMERIC => &["decimal_point", "thousands_sep", "grouping"],
        LC_TIME => &[
            "abday", "day", "abmon", "mon", "d_t_fmt", "d_fmt", "t_fmt", "am_pm",
        ],
        LC_COLLATE => &["order"],
        LC_MONETARY => &[
            "int_curr_symbol",
            "currency_symbol",
            "mon_decimal_point",
            "mon_thousands_sep",
            "mon_grouping",
            "positive_sign",
            "negative_sign",
            "int_frac_digits",
            "frac_digits",
            "p_cs_precedes",
            "p_sep_by_space",
            "n_cs_precedes",
            "n_sep_by_space",
            "p_sign_posn",
            "n_sign_posn",
        ],
        LC_MESSAGES => &["yesexpr", "noexpr", "yesstr", "nostr"],
        _ => return None,
    };
    table
        .iter()
        .position(|value| *value == keyword)
        .and_then(|index| i32::try_from(index).ok())
        .map(|index| index.saturating_add(1))
}

/// Parse one or more quoted values separated by `;`.
fn parse_values(rest: &str, line: usize) -> Result<Vec<u8>, LocaledefError> {
    let mut characters = rest.chars().peekable();
    let mut output = Vec::new();
    loop {
        while matches!(characters.peek(), Some(' ' | '\t')) {
            characters.next();
        }
        if characters.next() != Some('"') {
            return Err(LocaledefError::Malformed(format!(
                "line {line}: expected a quoted value"
            )));
        }
        loop {
            match characters.next() {
                Some('"') => break,
                Some('\\') => match characters.next() {
                    Some('\\') => output.push(b'\\'),
                    Some('"') => output.push(b'"'),
                    Some('n') => output.push(b'\n'),
                    Some('t') => output.push(b'\t'),
                    Some('r') => output.push(b'\r'),
                    _ => {
                        return Err(LocaledefError::Malformed(format!(
                            "line {line}: unsupported escape"
                        )));
                    }
                },
                Some(character) => {
                    let mut buffer = [0u8; 4];
                    output.extend_from_slice(character.encode_utf8(&mut buffer).as_bytes());
                }
                None => {
                    return Err(LocaledefError::Malformed(format!(
                        "line {line}: unterminated quoted value"
                    )));
                }
            }
        }
        while matches!(characters.peek(), Some(' ' | '\t')) {
            characters.next();
        }
        match characters.peek() {
            Some(';') => {
                characters.next();
                output.push(b';');
            }
            None => break,
            _ => {
                return Err(LocaledefError::Malformed(format!(
                    "line {line}: expected ; or end of line"
                )));
            }
        }
    }
    Ok(output)
}

/// Parse a bounded POSIX locale source.
pub fn parse_locale(source: &str) -> Result<Sets, LocaledefError> {
    let mut sets: Sets = BTreeMap::new();
    let mut current: Option<i32> = None;
    for (index, raw) in source.lines().enumerate() {
        let line = raw.trim();
        let number = index.saturating_add(1);
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some(set) = current {
            if line == format!("END {}", category_name(set)) {
                current = None;
                continue;
            }
            if line.starts_with("END ") {
                return Err(LocaledefError::Malformed(format!(
                    "line {number}: mismatched END"
                )));
            }
            if category(line).is_some() {
                return Err(LocaledefError::Malformed(format!(
                    "line {number}: nested category"
                )));
            }
            let Some((keyword, rest)) = line.split_once(char::is_whitespace) else {
                return Err(LocaledefError::Malformed(format!(
                    "line {number}: expected keyword and value"
                )));
            };
            let id = if keyword == "copy" {
                COPY_ID
            } else {
                keyword_id(set, keyword).ok_or_else(|| {
                    LocaledefError::Unsupported(format!("line {number}: {keyword}"))
                })?
            };
            let value = parse_values(rest.trim(), number)?;
            sets.entry(set).or_default().insert(id, value);
        } else if let Some(set) = category(line) {
            sets.entry(set).or_default();
            current = Some(set);
        } else if line.starts_with("END ") {
            return Err(LocaledefError::Malformed(format!(
                "line {number}: END without a category"
            )));
        } else {
            return Err(LocaledefError::Unsupported(format!(
                "line {number}: {line}"
            )));
        }
    }
    if let Some(set) = current {
        return Err(LocaledefError::Malformed(format!(
            "missing END {}",
            category_name(set)
        )));
    }
    if sets.is_empty() {
        return Err(LocaledefError::Malformed(
            "no locale categories".to_string(),
        ));
    }
    Ok(sets)
}

/// Parse a bounded POSIX charmap and return the entry count.
pub fn parse_charmap(source: &str) -> Result<usize, LocaledefError> {
    let mut count = 0usize;
    for (index, raw) in source.lines().enumerate() {
        let line = raw.trim();
        let number = index.saturating_add(1);
        if line.is_empty() || line.starts_with('#') || line.starts_with('%') {
            continue;
        }
        let mut fields = line.split_whitespace();
        let (Some(symbol), Some(bytes), Some(unicode)) =
            (fields.next(), fields.next(), fields.next())
        else {
            return Err(LocaledefError::Malformed(format!(
                "line {number}: expected <symbol> /xHH U+HHHH"
            )));
        };
        if !(symbol.starts_with('<') && symbol.ends_with('>') && symbol.len() > 2) {
            return Err(LocaledefError::Malformed(format!(
                "line {number}: bad symbol {symbol}"
            )));
        }
        let hex = bytes
            .strip_prefix("/x")
            .or_else(|| bytes.strip_prefix("\\x"));
        let Some(hex) = hex else {
            return Err(LocaledefError::Malformed(format!(
                "line {number}: bad byte code {bytes}"
            )));
        };
        if hex.is_empty() || !hex.chars().all(|character| character.is_ascii_hexdigit()) {
            return Err(LocaledefError::Malformed(format!(
                "line {number}: bad byte code {bytes}"
            )));
        }
        let Some(hex) = unicode.strip_prefix("U+") else {
            return Err(LocaledefError::Malformed(format!(
                "line {number}: bad code point {unicode}"
            )));
        };
        if hex.is_empty() || !hex.chars().all(|character| character.is_ascii_hexdigit()) {
            return Err(LocaledefError::Malformed(format!(
                "line {number}: bad code point {unicode}"
            )));
        }
        count = count.saturating_add(1);
    }
    if count == 0 {
        return Err(LocaledefError::Malformed("empty charmap".to_string()));
    }
    Ok(count)
}

/// Parse and compile a locale source and charmap into the runtime catalog image.
///
/// Every parse error is returned before any image is produced, so a caller never writes a
/// partial catalog as a success.
pub fn compile(
    source: &str,
    charmap_source: &str,
    charmap_name: &str,
    name: Option<&str>,
) -> Result<Vec<u8>, LocaledefError> {
    let mut sets = parse_locale(source)?;
    let entries = parse_charmap(charmap_source)?;
    let categories: Vec<String> = sets
        .keys()
        .filter(|set| **set >= LC_CTYPE && **set <= LC_MESSAGES)
        .map(|set| category_name(*set).to_string())
        .collect();
    let metadata = sets.entry(LOCALE).or_default();
    if let Some(name) = name {
        metadata.insert(LOCALE_NAME_ID, name.as_bytes().to_vec());
    }
    metadata.insert(LOCALE_CHARMAP_ID, charmap_name.as_bytes().to_vec());
    metadata.insert(LOCALE_CHARMAP_ENTRIES_ID, entries.to_string().into_bytes());
    metadata.insert(LOCALE_CATEGORIES_ID, categories.join(",").into_bytes());
    write_catalog(&sets).map_err(LocaledefError::Malformed)
}

#[cfg(test)]
#[path = "lib_ut.rs"]
mod tests;
