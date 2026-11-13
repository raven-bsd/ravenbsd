// SPDX-License-Identifier: MIT
// RavenBSD: transpiled from the pinned donor; see PROVENANCE.toml.
// Copyright (c) 2005-2020 Rich Felker, et al.
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// RavenBSD: the musl charmaps alias decoder from src/locale/iconv.c.

//! Charset names, alias matching and the 8-bit tables.

use crate::tables_8bit::{CHARMAPS, LEGACY_CHARS};

pub(crate) const UTF_32BE: u8 = 0o300;
pub(crate) const UTF_16LE: u8 = 0o301;
pub(crate) const UTF_16BE: u8 = 0o302;
pub(crate) const UTF_32LE: u8 = 0o303;
pub(crate) const UCS2BE: u8 = 0o304;
pub(crate) const UCS2LE: u8 = 0o305;
pub(crate) const WCHAR_T: u8 = 0o306;
pub(crate) const US_ASCII: u8 = 0o307;
pub(crate) const UTF_8: u8 = 0o310;
pub(crate) const UTF_16: u8 = 0o312;
pub(crate) const UTF_32: u8 = 0o313;
pub(crate) const UCS2: u8 = 0o314;
pub(crate) const EUC_JP: u8 = 0o320;
pub(crate) const SHIFT_JIS: u8 = 0o321;
pub(crate) const ISO2022_JP: u8 = 0o322;
pub(crate) const GB18030: u8 = 0o330;
pub(crate) const GBK: u8 = 0o331;
pub(crate) const GB2312: u8 = 0o332;
pub(crate) const BIG5: u8 = 0o340;
pub(crate) const EUC_KR: u8 = 0o350;

/// A resolved source/target charset.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Charset {
    /// UTF-8.
    Utf8,
    /// UTF-16 with BOM detection.
    Utf16,
    /// Explicit little-endian UTF-16.
    Utf16Le,
    /// Explicit big-endian UTF-16.
    Utf16Be,
    /// UTF-32 with BOM detection.
    Utf32,
    /// Explicit little-endian UTF-32.
    Utf32Le,
    /// Explicit big-endian UTF-32.
    Utf32Be,
    /// UCS-2 with BOM detection.
    Ucs2,
    /// Explicit little-endian UCS-2.
    Ucs2Le,
    /// Explicit big-endian UCS-2.
    Ucs2Be,
    /// Native `wchar_t` (32-bit).
    WcharT,
    /// ASCII.
    Ascii,
    /// A legacy 8-bit charset at this `CHARMAPS` offset.
    Legacy(usize),
    /// A CJK charset.
    Cjk(Cjk),
}

/// The CJK charsets.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Cjk {
    /// EUC-JP.
    EucJp,
    /// Shift-JIS.
    ShiftJis,
    /// ISO-2022-JP.
    Iso2022Jp,
    /// GB18030.
    Gb18030,
    /// GBK.
    Gbk,
    /// GB2312.
    Gb2312,
    /// Big5.
    Big5,
    /// EUC-KR.
    EucKr,
}

fn normalise(name: &[u8]) -> alloc::vec::Vec<u8> {
    let mut out = alloc::vec::Vec::new();
    for byte in name {
        if byte.is_ascii_alphanumeric() {
            out.push(byte.to_ascii_lowercase());
        }
    }
    out
}

fn alias_matches(query: &[u8], alias: &[u8]) -> bool {
    normalise(query) == normalise(alias)
}

/// Resolve a charset name to `(type code, 8-bit table offset)`.
///
/// The special names live in `iconv.c`'s prefix of `charmaps`, while
/// `codepages.h` starts with the 8-bit aliases; both are covered here.
pub(crate) fn resolve(name: &[u8]) -> Option<(u8, usize)> {
    let normalized = normalise(name);
    let special: &[(&[u8], u8)] = &[
        (b"utf8", UTF_8),
        (b"char", UTF_8),
        (b"wchart", WCHAR_T),
        (b"wchar_t", WCHAR_T),
        (b"ucs2be", UCS2BE),
        (b"ucs2le", UCS2LE),
        (b"utf16be", UTF_16BE),
        (b"utf16le", UTF_16LE),
        (b"ucs4be", UTF_32BE),
        (b"utf32be", UTF_32BE),
        (b"ucs4le", UTF_32LE),
        (b"utf32le", UTF_32LE),
        (b"ascii", US_ASCII),
        (b"usascii", US_ASCII),
        (b"iso646", US_ASCII),
        (b"iso646us", US_ASCII),
        (b"utf16", UTF_16),
        (b"ucs4", UTF_32),
        (b"utf32", UTF_32),
        (b"ucs2", UCS2),
        (b"eucjp", EUC_JP),
        (b"shiftjis", SHIFT_JIS),
        (b"sjis", SHIFT_JIS),
        (b"cp932", SHIFT_JIS),
        (b"iso2022jp", ISO2022_JP),
        (b"gb18030", GB18030),
        (b"gbk", GBK),
        (b"cp936", GBK),
        (b"windows936", GBK),
        (b"gb2312", GB2312),
        (b"big5", BIG5),
        (b"bigfive", BIG5),
        (b"cp950", BIG5),
        (b"big5hkscs", BIG5),
        (b"euckr", EUC_KR),
        (b"ksc5601", EUC_KR),
        (b"ksx1001", EUC_KR),
        (b"cp949", EUC_KR),
    ];
    if normalized.is_empty() {
        return Some((UTF_8, 0));
    }
    if matches!(
        normalized.as_slice(),
        b"ansix341968" | b"ansix341986" | b"us" | b"csascii" | b"iso646irv1991"
    ) {
        return Some((US_ASCII, 0));
    }
    let legacy_alias = match normalized.as_slice() {
        b"l1" => Some(b"latin1".as_slice()),
        b"l2" => Some(b"iso88592".as_slice()),
        b"l9" => Some(b"iso885915".as_slice()),
        _ => None,
    };
    if let Some(alias) = legacy_alias {
        let offset = find_charmap(alias)?;
        return Some((type_code(offset), offset));
    }
    for (alias, kind) in special {
        if normalized == *alias {
            return Some((*kind, 0));
        }
    }
    find_charmap(name).map(|offset| (type_code(offset), offset))
}

/// Find a charset by fuzzy name; the returned offset points at the type byte.
pub(crate) fn find_charmap(name: &[u8]) -> Option<usize> {
    let query = if name.is_empty() { b"utf8" } else { name };
    let mut position = 0usize;
    while position < CHARMAPS.len() {
        let end = CHARMAPS
            .get(position..)
            .and_then(|rest| rest.iter().position(|byte| *byte == 0))?;
        let alias = CHARMAPS.get(position..position.checked_add(end)?)?;
        if !alias.is_empty() && alias_matches(query, alias) {
            let mut cursor = position;
            loop {
                let length = CHARMAPS.get(cursor..)?.iter().position(|byte| *byte == 0)?;
                if length == 0 {
                    return cursor.checked_add(1);
                }
                cursor = cursor.checked_add(length)?.checked_add(1)?;
            }
        }
        position = position.checked_add(end)?.checked_add(1)?;
        if CHARMAPS.get(position) == Some(&0) {
            let kind = CHARMAPS.get(position.checked_add(1)?).copied()?;
            position = if kind > 0o200 {
                position.checked_add(2)?
            } else {
                position.checked_add(2)?.checked_add(
                    (64u16.saturating_sub(u16::from(kind))).saturating_mul(5) as usize,
                )?
            };
        }
    }
    None
}

/// The type byte preceding a charmap offset.
#[must_use]
pub(crate) fn type_code(offset: usize) -> u8 {
    CHARMAPS.get(offset).copied().unwrap_or(0)
}

/// Resolve a type byte to a [`Charset`].
#[must_use]
pub(crate) fn from_type(kind: u8, offset: usize) -> Charset {
    match kind {
        UTF_8 => Charset::Utf8,
        UTF_16 => Charset::Utf16,
        UTF_16LE => Charset::Utf16Le,
        UTF_16BE => Charset::Utf16Be,
        UTF_32 => Charset::Utf32,
        UTF_32LE => Charset::Utf32Le,
        UTF_32BE => Charset::Utf32Be,
        UCS2 => Charset::Ucs2,
        UCS2LE => Charset::Ucs2Le,
        UCS2BE => Charset::Ucs2Be,
        WCHAR_T => Charset::WcharT,
        US_ASCII => Charset::Ascii,
        EUC_JP => Charset::Cjk(Cjk::EucJp),
        SHIFT_JIS => Charset::Cjk(Cjk::ShiftJis),
        ISO2022_JP => Charset::Cjk(Cjk::Iso2022Jp),
        GB18030 => Charset::Cjk(Cjk::Gb18030),
        GBK => Charset::Cjk(Cjk::Gbk),
        GB2312 => Charset::Cjk(Cjk::Gb2312),
        BIG5 => Charset::Cjk(Cjk::Big5),
        EUC_KR => Charset::Cjk(Cjk::EucKr),
        _ => Charset::Legacy(offset),
    }
}

/// Decode one byte through an 8-bit charmap.
#[must_use]
pub(crate) fn legacy_map(offset: usize, byte: u8) -> u32 {
    let Some(map) = CHARMAPS.get(offset.saturating_add(1)..) else {
        return 0xffff;
    };
    let kind = type_code(offset);
    let c = u32::from(byte);
    let first = u32::from(kind).saturating_mul(4);
    if c < first {
        return c;
    }
    let index = c.saturating_sub(first);
    let shift = (index % 8).saturating_mul(2);
    let at = (index.saturating_mul(5) / 4) as usize;
    let Some(low) = map.get(at).copied() else {
        return 0xffff;
    };
    let high = map.get(at.saturating_add(1)).copied().unwrap_or(0);
    let value = (u32::from(low) >> shift) | (u32::from(high) << (8 - shift));
    let value = value & 1023;
    if value < 256 {
        value
    } else {
        LEGACY_CHARS
            .get((value - 256) as usize)
            .copied()
            .map_or(0xffff, u32::from)
    }
}

/// Whether a type code is one of the 8-bit legacy tables.
#[must_use]
pub(crate) fn is_legacy(kind: u8) -> bool {
    kind < 0o200
}

/// Whether the target is forbidden by the donor (`charmaps[t] >= 0330`).
#[must_use]
pub(crate) fn forbidden_target(kind: u8) -> bool {
    kind >= 0o330
}

#[cfg(test)]
#[path = "charset_ut.rs"]
mod tests;
