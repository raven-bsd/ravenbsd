// SPDX-License-Identifier: MIT
// RavenBSD: transpiled from the pinned donor; see PROVENANCE.toml.
// Copyright (c) 2005-2020 Rich Felker, et al.
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// RavenBSD: the XSI catalog reader from musl:src/locale/{catopen,catgets,catclose}.c @
// 7813e9119eb5fe681b6750b8a033a0490bca0941.

//! XSI `catopen`/`catgets` over the `gencat` format.
#![allow(clippy::indexing_slicing)] // RavenBSD: the index is bounded by the preceding size check.

use alloc::vec::Vec;

use crate::gettext::FileSource;
use crate::locale::EnvSource;

/// A catalog failure.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum CatError {
    /// The file or path could not be read.
    NotFound,
    /// The magic or size header is wrong.
    BadFormat,
}

/// A parsed catalog image.
pub struct Catalog {
    data: Vec<u8>,
}

fn read_u32(data: &[u8], offset: usize) -> Option<u32> {
    let bytes = data.get(offset..offset.checked_add(4)?)?;
    Some(u32::from_be_bytes([
        *bytes.first()?,
        *bytes.get(1)?,
        *bytes.get(2)?,
        *bytes.get(3)?,
    ]))
}

impl Catalog {
    /// Open a catalog.
    pub fn open(
        fs: &dyn FileSource,
        env: &dyn EnvSource,
        name: &[u8],
        oflag: i32,
        secure: bool,
        loc_name: &[u8],
    ) -> Result<Self, CatError> {
        let mut candidates = alloc::vec::Vec::new();
        if name.contains(&b'/') {
            candidates.push(name.to_vec());
        } else if !secure && let Some(path) = env.get(b"NLSPATH") {
            for pattern in path.split(|byte| *byte == b':') {
                let expanded = expand(pattern, name, loc_name, oflag);
                if !expanded.is_empty() {
                    candidates.push(expanded);
                }
            }
        }
        for path in candidates {
            let Some(data) = fs.read_all(&path, 8 * 1024 * 1024) else {
                continue;
            };
            if data.len() < 20 || read_u32(&data, 0) != Some(0xff88_ff89) {
                continue;
            }
            let Some(offset) = read_u32(&data, 8) else {
                continue;
            };
            if 20usize.saturating_add(offset as usize) != data.len() {
                continue;
            }
            return Ok(Self { data });
        }
        Err(CatError::NotFound)
    }

    /// `catgets`; `default` is returned for a missing set or message.
    #[must_use]
    pub fn gets<'a>(&'a self, set: i32, msg: i32, default: &'a [u8]) -> &'a [u8] {
        let Some(nsets) = read_u32(&self.data, 4) else {
            return default;
        };
        let sets = 20usize;
        let Some(strings_offset) = read_u32(&self.data, 16) else {
            return default;
        };
        let Some(msgs_table_offset) = read_u32(&self.data, 12) else {
            return default;
        };
        let strings = sets.saturating_add(strings_offset as usize);
        let msgs_table = sets.saturating_add(msgs_table_offset as usize);
        for index in 0..nsets as usize {
            let base = sets.saturating_add(index.saturating_mul(12));
            let Some(id) = read_u32(&self.data, base) else {
                return default;
            };
            if id as i32 != set {
                continue;
            }
            let Some(nmsgs) = read_u32(&self.data, base.saturating_add(4)) else {
                return default;
            };
            let Some(msgs_offset) = read_u32(&self.data, base.saturating_add(8)) else {
                return default;
            };
            let msgs = msgs_table.saturating_add(12usize.saturating_mul(msgs_offset as usize));
            for msg_index in 0..nmsgs as usize {
                let msg_base = msgs.saturating_add(msg_index.saturating_mul(12));
                let Some(msg_id) = read_u32(&self.data, msg_base) else {
                    return default;
                };
                if msg_id as i32 != msg {
                    continue;
                }
                let Some(length) = read_u32(&self.data, msg_base.saturating_add(4)) else {
                    return default;
                };
                let Some(string_offset) = read_u32(&self.data, msg_base.saturating_add(8)) else {
                    return default;
                };
                let start = strings.saturating_add(string_offset as usize);
                let end = start.saturating_add(length as usize);
                return self.data.get(start..end).unwrap_or(default);
            }
            return default;
        }
        default
    }
}

fn expand(pattern: &[u8], name: &[u8], loc_name: &[u8], oflag: i32) -> Vec<u8> {
    let mut out = Vec::new();
    let language = if oflag != 0 { loc_name } else { &b""[..] };
    let mut index = 0usize;
    while let Some(byte) = pattern.get(index).copied() {
        index += 1;
        if byte != b'%' {
            out.push(byte);
            continue;
        }
        let Some(code) = pattern.get(index).copied() else {
            break;
        };
        index += 1;
        match code {
            b'N' => out.extend_from_slice(name),
            b'L' | b'l' => {
                out.extend_from_slice(language);
                if code == b'l' {
                    let cut = language
                        .iter()
                        .position(|byte| matches!(byte, b'_' | b'.' | b'@'))
                        .unwrap_or(language.len());
                    out.truncate(out.len().saturating_sub(language.len()).saturating_add(cut));
                }
            }
            b't' => {
                let rest = language
                    .iter()
                    .position(|byte| *byte == b'_')
                    .and_then(|position| language.get(position.saturating_add(1)..))
                    .unwrap_or(b"");
                let cut = rest
                    .iter()
                    .position(|byte| matches!(byte, b'.' | b'@'))
                    .unwrap_or(rest.len());
                out.extend_from_slice(rest.get(..cut).unwrap_or(b""));
            }
            b'c' => out.extend_from_slice(b"UTF-8"),
            b'%' => out.push(b'%'),
            _ => return Vec::new(),
        }
    }
    out
}

#[cfg(test)]
#[path = "catalog_ut.rs"]
mod tests;
