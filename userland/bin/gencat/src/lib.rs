// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Compile the POSIX `gencat` source syntax into the catalog image read by
//! `raven_libc_locale::nls::Catalog`.
//!
//! Supported source lines are `$set N`, `$delset N`, `$transient`, `$comment` and
//! `<msgid> "<string>"`. Strings are UTF-8 and support `\\`, `\"`, `\n`, `\t` and `\r`.
//! An existing catalog is replaced, not merged.

#![cfg_attr(target_os = "none", no_std)]
#![forbid(unsafe_code)]
#![warn(missing_docs)]

extern crate alloc;

use alloc::collections::BTreeMap;
use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;

/// Maximum catalog image size accepted by the runtime reader.
pub const MAX_CATALOG: usize = 8 * 1024 * 1024;

/// Sets of messages: set id -> message id -> bytes.
pub type Sets = BTreeMap<i32, BTreeMap<i32, Vec<u8>>>;

fn parse_id(value: &str, line: usize, what: &str) -> Result<i32, String> {
    let id = value
        .parse::<i32>()
        .map_err(|_| format!("line {line}: invalid {what} id `{value}`"))?;
    if id <= 0 {
        return Err(format!("line {line}: {what} id must be positive"));
    }
    Ok(id)
}

fn parse_quoted(text: &str, line: usize) -> Result<Vec<u8>, String> {
    if !text.starts_with('"') || !text.ends_with('"') || text.len() < 2 {
        return Err(format!("line {line}: message must be a quoted string"));
    }
    let inner = &text[1..text.len() - 1];
    let mut value = String::new();
    let mut characters = inner.chars();
    while let Some(character) = characters.next() {
        if character == '"' {
            return Err(format!("line {line}: unescaped quote in message"));
        }
        if character != '\\' {
            value.push(character);
            continue;
        }
        let Some(escape) = characters.next() else {
            return Err(format!("line {line}: trailing backslash"));
        };
        match escape {
            '\\' => value.push('\\'),
            '"' => value.push('"'),
            'n' => value.push('\n'),
            't' => value.push('\t'),
            'r' => value.push('\r'),
            other => {
                return Err(format!("line {line}: unsupported escape `\\{other}`"));
            }
        }
    }
    Ok(value.into_bytes())
}

fn parse_into(source: &str, sets: &mut Sets, current: &mut Option<i32>) -> Result<(), String> {
    for (index, raw) in source.lines().enumerate() {
        let line_number = index + 1;
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let mut parts = line.split_whitespace();
        match parts.next() {
            Some("$set") => {
                let id = parts
                    .next()
                    .ok_or_else(|| format!("line {line_number}: $set needs a set id"))?;
                let id = parse_id(id, line_number, "set")?;
                if parts.next().is_some() {
                    return Err(format!("line {line_number}: $set takes one id"));
                }
                sets.entry(id).or_default();
                *current = Some(id);
            }
            Some("$delset") => {
                let id = parts
                    .next()
                    .ok_or_else(|| format!("line {line_number}: $delset needs a set id"))?;
                let id = parse_id(id, line_number, "set")?;
                sets.remove(&id);
                if *current == Some(id) {
                    *current = None;
                }
            }
            Some("$transient") | Some("$comment") => {}
            Some(value) => {
                let Some(set) = *current else {
                    return Err(format!("line {line_number}: message before a $set"));
                };
                let rest = line
                    .split_once(char::is_whitespace)
                    .map(|(_, rest)| rest.trim_start())
                    .ok_or_else(|| format!("line {line_number}: message needs a quoted string"))?;
                let id = parse_id(value, line_number, "message")?;
                let text = parse_quoted(rest, line_number)?;
                sets.entry(set).or_default().insert(id, text);
            }
            None => {}
        }
    }
    Ok(())
}

/// Parse one or more `gencat` source files into the in-memory set model.
pub fn parse_sources(sources: &[&str]) -> Result<Sets, String> {
    let mut sets = Sets::new();
    let mut current = None;
    for source in sources {
        parse_into(source, &mut sets, &mut current)?;
    }
    Ok(sets)
}

fn push_u32(output: &mut Vec<u8>, value: usize) -> Result<(), String> {
    let value = u32::try_from(value).map_err(|_| "catalog is too large".to_string())?;
    output.extend_from_slice(&value.to_be_bytes());
    Ok(())
}

/// Build the binary catalog image.
pub fn write_catalog(sets: &Sets) -> Result<Vec<u8>, String> {
    let nsets = sets.len();
    let messages = sets.values().map(|messages| messages.len()).sum::<usize>();
    let message_table_offset = nsets
        .checked_mul(12)
        .ok_or_else(|| "catalog is too large".to_string())?;
    let strings_offset = message_table_offset
        .checked_add(
            messages
                .checked_mul(12)
                .ok_or_else(|| "catalog is too large".to_string())?,
        )
        .ok_or_else(|| "catalog is too large".to_string())?;

    let mut output = Vec::new();
    push_u32(&mut output, 0xff88_ff89)?;
    push_u32(&mut output, nsets)?;
    push_u32(&mut output, 0)?;
    push_u32(&mut output, message_table_offset)?;
    push_u32(&mut output, strings_offset)?;

    let mut message_index = 0usize;
    for (set, messages) in sets {
        push_u32(&mut output, *set as usize)?;
        push_u32(&mut output, messages.len())?;
        push_u32(&mut output, message_index)?;
        message_index = message_index
            .checked_add(messages.len())
            .ok_or_else(|| "catalog is too large".to_string())?;
    }

    let mut string_offset = 0usize;
    for messages in sets.values() {
        for (id, text) in messages {
            push_u32(&mut output, *id as usize)?;
            push_u32(&mut output, text.len())?;
            push_u32(&mut output, string_offset)?;
            string_offset = string_offset
                .checked_add(text.len())
                .and_then(|value| value.checked_add(1))
                .ok_or_else(|| "catalog is too large".to_string())?;
        }
    }

    for messages in sets.values() {
        for text in messages.values() {
            output.extend_from_slice(text);
            output.push(0);
        }
    }
    if output.len() > MAX_CATALOG {
        return Err("catalog exceeds 8 MiB".to_string());
    }
    let size = output
        .len()
        .checked_sub(20)
        .ok_or_else(|| "catalog is too small".to_string())?;
    let bytes = u32::try_from(size).map_err(|_| "catalog is too large".to_string())?;
    output[8..12].copy_from_slice(&bytes.to_be_bytes());
    Ok(output)
}

/// Parse and compile all sources in order.
pub fn compile(sources: &[&str]) -> Result<Vec<u8>, String> {
    write_catalog(&parse_sources(sources)?)
}

#[cfg(test)]
#[path = "lib_ut.rs"]
mod tests;
