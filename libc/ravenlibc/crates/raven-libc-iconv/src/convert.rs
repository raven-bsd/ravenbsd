// SPDX-License-Identifier: MIT
// RavenBSD: transpiled from the pinned donor; see PROVENANCE.toml.
// Copyright (c) 2005-2020 Rich Felker, et al.
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// RavenBSD: the iconv main loop from musl:src/locale/iconv.c @
// 7813e9119eb5fe681b6750b8a033a0490bca0941. P1 strips //TRANSLIT//IGNORE
// before lookup; P2 uses the strict/translit/ignore outcome table; P3 calls
// the C39 UTF-8 codec directly.

//! The converter state machine.
#![allow(clippy::indexing_slicing)] // RavenBSD: the index is bounded by the preceding size check.

use raven_libc_ctype::{MbMode, MbState, Mbr, mbrtowc, wcrtomb};

use crate::charset::{Charset, Cjk, forbidden_target, from_type, is_legacy, legacy_map, resolve};
use crate::{IconvError, Mode};

#[cfg(feature = "cjk")]
use raven_libc_iconv_cjk::{
    BIG5 as BIG5_TABLE, GB18030 as GB18030_TABLE, GB18030UTF, JIS0208, KSC, REVJIS,
};

/// A conversion descriptor.
pub struct Iconv {
    from: Charset,
    to: Charset,
    mode: Mode,
    bom_state: u32,
    iso_state: u32,
    utf8_state: MbState,
}

impl Iconv {
    /// `iconv_open`.
    pub fn open(to: &[u8], from: &[u8]) -> Result<Self, IconvError> {
        let (to_name, to_mode) = strip_mode(to);
        let (from_name, from_mode) = strip_mode(from);
        let (to_type, to_offset) = resolve(to_name).ok_or(IconvError::Unsupported)?;
        let (from_type, from_offset) = resolve(from_name).ok_or(IconvError::Unsupported)?;
        #[cfg(not(feature = "cjk"))]
        if matches!(from_type_from(from_type, from_offset), Charset::Cjk(_))
            || matches!(from_type_from(to_type, to_offset), Charset::Cjk(_))
        {
            return Err(IconvError::Unsupported);
        }
        if forbidden_target(to_type) {
            return Err(IconvError::Unsupported);
        }
        let mode = if to_mode == Mode::Translit || from_mode == Mode::Translit {
            Mode::Translit
        } else if to_mode == Mode::Ignore || from_mode == Mode::Ignore {
            Mode::Ignore
        } else {
            Mode::Strict
        };
        Ok(Self {
            from: from_type_from(from_type, from_offset),
            to: from_type_from(to_type, to_offset),
            mode,
            bom_state: 0,
            iso_state: 0,
            utf8_state: MbState::INITIAL,
        })
    }

    /// `iconv`.
    pub fn convert(
        &mut self,
        input: &mut &[u8],
        output: &mut &mut [u8],
    ) -> Result<usize, IconvError> {
        let mut irreversible = 0usize;
        let mut ignored = false;
        loop {
            let slice = *input;
            if slice.is_empty() {
                if ignored {
                    return Err(IconvError::IllegalSequence);
                }
                return Ok(irreversible);
            }
            let (code, used) = match self.decode(slice) {
                Ok(value) => value,
                Err(error) => {
                    *input = slice;
                    return Err(error);
                }
            };
            let mut buffer = [0u8; 8];
            match self.encode(code, &mut buffer) {
                Ok((length, additions)) => {
                    let available = core::mem::take(output);
                    if available.len() < length {
                        *output = available;
                        return Err(IconvError::TooBig);
                    }
                    if let Some(destination) = available.get_mut(..length)
                        && let Some(source) = buffer.get(..length)
                    {
                        destination.copy_from_slice(source);
                    }
                    *output = available.get_mut(length..).unwrap_or(&mut []);
                    *input = slice.get(used..).unwrap_or(&[]);
                    irreversible = irreversible.saturating_add(additions);
                }
                Err(()) => match self.mode {
                    Mode::Strict => {
                        *input = slice;
                        return Err(IconvError::IllegalSequence);
                    }
                    Mode::Translit => {
                        let available = core::mem::take(output);
                        if available.is_empty() {
                            *output = available;
                            return Err(IconvError::TooBig);
                        }
                        if let Some(destination) = available.get_mut(..1) {
                            destination[0] = b'?';
                        }
                        *output = available.get_mut(1..).unwrap_or(&mut []);
                        *input = slice.get(used..).unwrap_or(&[]);
                        irreversible = irreversible.saturating_add(1);
                    }
                    Mode::Ignore => {
                        *input = slice.get(used..).unwrap_or(&[]);
                        ignored = true;
                    }
                },
            }
        }
    }

    /// `iconv(cd, NULL, ...)`.
    pub fn reset(&mut self) {
        self.bom_state = 0;
        self.iso_state = 0;
        self.utf8_state = MbState::INITIAL;
    }

    /// Whether the descriptor needs heap state in the shell.
    #[must_use]
    pub fn is_stateful(&self) -> bool {
        matches!(
            self.from,
            Charset::Utf16 | Charset::Utf32 | Charset::Ucs2 | Charset::Cjk(Cjk::Iso2022Jp)
        )
    }

    fn decode(&mut self, input: &[u8]) -> Result<(u32, usize), IconvError> {
        match self.from {
            Charset::Utf8 => self.decode_utf8(input),
            Charset::Ascii => decode_ascii(input),
            Charset::Legacy(offset) => decode_legacy(offset, input),
            Charset::Utf16 => self.decode_utf16_bom(input),
            Charset::Utf16Le => decode_utf16(input, true, false),
            Charset::Utf16Be => decode_utf16(input, false, false),
            Charset::Ucs2 => self.decode_ucs2_bom(input),
            Charset::Ucs2Le => decode_utf16(input, true, true),
            Charset::Ucs2Be => decode_utf16(input, false, true),
            Charset::Utf32 => self.decode_utf32_bom(input),
            Charset::Utf32Le => decode_utf32(input, true),
            Charset::Utf32Be => decode_utf32(input, false),
            Charset::WcharT => decode_utf32(input, cfg!(target_endian = "little")),
            Charset::Cjk(cjk) => self.decode_cjk(cjk, input),
        }
    }

    fn decode_utf8(&mut self, input: &[u8]) -> Result<(u32, usize), IconvError> {
        match mbrtowc(&mut self.utf8_state, MbMode::Utf8, input) {
            Mbr::Char { wc, used } => Ok((wc, used)),
            Mbr::Incomplete => Err(IconvError::Incomplete),
            Mbr::Ilseq => Err(IconvError::IllegalSequence),
        }
    }

    fn decode_utf16_bom(&mut self, input: &[u8]) -> Result<(u32, usize), IconvError> {
        if self.bom_state == 0 {
            let Some(bytes) = input.get(..2) else {
                return Err(IconvError::Incomplete);
            };
            let first = u16::from_be_bytes([bytes[0], bytes[1]]);
            self.bom_state = if first == 0xfffe { 1 } else { 2 };
            let skip = usize::from(first == 0xfffe || first == 0xfeff) * 2;
            if skip == 2 {
                let (code, used) = self.decode_utf16_bom(input.get(2..).unwrap_or(&[]))?;
                return Ok((code, used + 2));
            }
        }
        decode_utf16(input, self.bom_state == 1, false)
    }

    fn decode_ucs2_bom(&mut self, input: &[u8]) -> Result<(u32, usize), IconvError> {
        if self.bom_state == 0 {
            let Some(bytes) = input.get(..2) else {
                return Err(IconvError::Incomplete);
            };
            let first = u16::from_be_bytes([bytes[0], bytes[1]]);
            self.bom_state = if first == 0xfffe { 1 } else { 2 };
            let skip = usize::from(first == 0xfffe || first == 0xfeff) * 2;
            if skip == 2 {
                let (code, used) = self.decode_ucs2_bom(input.get(2..).unwrap_or(&[]))?;
                return Ok((code, used + 2));
            }
        }
        decode_utf16(input, self.bom_state == 1, true)
    }

    fn decode_utf32_bom(&mut self, input: &[u8]) -> Result<(u32, usize), IconvError> {
        if self.bom_state == 0 {
            let Some(bytes) = input.get(..4) else {
                return Err(IconvError::Incomplete);
            };
            let first = u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
            self.bom_state = if first == 0xfffe_0000 { 1 } else { 2 };
            let skip = usize::from(first == 0xfffe_0000 || first == 0xfeff) * 4;
            if skip == 4 {
                let (code, used) = self.decode_utf32_bom(input.get(4..).unwrap_or(&[]))?;
                return Ok((code, used + 4));
            }
        }
        decode_utf32(input, self.bom_state == 1)
    }

    fn decode_cjk(&mut self, cjk: Cjk, input: &[u8]) -> Result<(u32, usize), IconvError> {
        #[cfg(not(feature = "cjk"))]
        {
            let _ = cjk;
            let _ = input;
            Err(IconvError::Unsupported)
        }
        #[cfg(feature = "cjk")]
        {
            match cjk {
                Cjk::EucJp => decode_euc_jp(input),
                Cjk::ShiftJis => decode_shift_jis(input),
                Cjk::Iso2022Jp => self.decode_iso2022_jp(input),
                Cjk::Gb18030 | Cjk::Gbk | Cjk::Gb2312 => decode_gb(cjk, input),
                Cjk::Big5 => decode_big5(input),
                Cjk::EucKr => decode_euc_kr(input),
            }
        }
    }

    fn encode(&mut self, code: u32, out: &mut [u8; 8]) -> Result<(usize, usize), ()> {
        match self.to {
            Charset::Utf8 => encode_utf8(code, out),
            Charset::Ascii => {
                if code > 0x7f {
                    return Err(());
                }
                if let Some(slot) = out.get_mut(0) {
                    *slot = code as u8;
                }
                Ok((1, 0))
            }
            Charset::Legacy(offset) => encode_legacy(offset, code, out),
            Charset::Utf16 | Charset::Utf16Be => encode_utf16(code, out, false, false),
            Charset::Utf16Le => encode_utf16(code, out, true, false),
            Charset::Ucs2 | Charset::Ucs2Be => encode_utf16(code, out, false, true),
            Charset::Ucs2Le => encode_utf16(code, out, true, true),
            Charset::Utf32 | Charset::Utf32Be => encode_utf32(code, out, false),
            Charset::Utf32Le => encode_utf32(code, out, true),
            Charset::WcharT => encode_utf32(code, out, cfg!(target_endian = "little")),
            Charset::Cjk(cjk) => encode_cjk(cjk, code, out),
        }
    }

    #[cfg(feature = "cjk")]
    fn decode_iso2022_jp(&mut self, input: &[u8]) -> Result<(u32, usize), IconvError> {
        let first = *input.first().ok_or(IconvError::Incomplete)?;
        if first >= 0x80 {
            return Err(IconvError::IllegalSequence);
        }
        if first == 0x1b {
            let bytes = input.get(..3).ok_or(IconvError::Incomplete)?;
            let second = bytes.get(1).copied().unwrap_or(0);
            let third = bytes.get(2).copied().unwrap_or(0);
            self.iso_state = match (second, third) {
                (b'(', b'B') => 0,
                (b'(', b'J') => 1,
                (b'(', b'I') => 4,
                (b'$', b'@') => 2,
                (b'$', b'B') => 3,
                _ => return Err(IconvError::IllegalSequence),
            };
            return Ok((0xfffe, 3));
        }
        let mut cursor = 0usize;
        let mut code = u32::from(first);
        match self.iso_state {
            1 => {
                if code == u32::from(b'\\') {
                    code = 0xa5;
                } else if code == u32::from(b'~') {
                    code = 0x203e;
                }
            }
            2 | 3 => {
                let bytes = input.get(..2).ok_or(IconvError::Incomplete)?;
                let second = u32::from(bytes.get(1).copied().unwrap_or(0));
                let row = code.saturating_sub(0x21);
                let column = second.saturating_sub(0x21);
                code = jis(row, column).ok_or(IconvError::IllegalSequence)?;
                cursor = 2;
            }
            4 => {
                if !(0x21..=0x5f).contains(&code) {
                    return Err(IconvError::IllegalSequence);
                }
                code = code.saturating_add(0xff61 - 0x21);
            }
            _ => {}
        }
        Ok((code, cursor.max(1)))
    }
}

fn strip_mode(name: &[u8]) -> (&[u8], Mode) {
    let mut current = name;
    let mut mode = Mode::Strict;
    loop {
        if let Some(rest) = current.strip_suffix(b"//TRANSLIT") {
            current = rest;
            mode = Mode::Translit;
        } else if let Some(rest) = current.strip_suffix(b"//IGNORE") {
            current = rest;
            if mode == Mode::Strict {
                mode = Mode::Ignore;
            }
        } else {
            break;
        }
    }
    (current, mode)
}

fn decode_ascii(input: &[u8]) -> Result<(u32, usize), IconvError> {
    let byte = *input.first().ok_or(IconvError::Incomplete)?;
    if byte >= 0x80 {
        return Err(IconvError::IllegalSequence);
    }
    Ok((u32::from(byte), 1))
}

fn decode_legacy(offset: usize, input: &[u8]) -> Result<(u32, usize), IconvError> {
    let byte = *input.first().ok_or(IconvError::Incomplete)?;
    let code = legacy_map(offset, byte);
    if code == 0xffff {
        return Err(IconvError::IllegalSequence);
    }
    Ok((code, 1))
}

fn get_16(bytes: &[u8], little: bool) -> u16 {
    let first = bytes.first().copied().unwrap_or(0);
    let second = bytes.get(1).copied().unwrap_or(0);
    if little {
        u16::from_le_bytes([first, second])
    } else {
        u16::from_be_bytes([first, second])
    }
}

fn decode_utf16(input: &[u8], little: bool, ucs2: bool) -> Result<(u32, usize), IconvError> {
    let bytes = input.get(..2).ok_or(IconvError::Incomplete)?;
    let first = get_16(bytes, little);
    if (0xdc00..0xe000).contains(&first) {
        return Err(IconvError::IllegalSequence);
    }
    if (0xd800..0xdc00).contains(&first) {
        if ucs2 {
            return Err(IconvError::IllegalSequence);
        }
        let rest = input.get(2..).ok_or(IconvError::Incomplete)?;
        let second_bytes = rest.get(..2).ok_or(IconvError::Incomplete)?;
        let second = get_16(second_bytes, little);
        if !(0xdc00..0xe000).contains(&second) {
            return Err(IconvError::IllegalSequence);
        }
        let code = 0x10000 + ((u32::from(first) - 0xd800) << 10) + (u32::from(second) - 0xdc00);
        return Ok((code, 4));
    }
    Ok((u32::from(first), 2))
}

fn decode_utf32(input: &[u8], little: bool) -> Result<(u32, usize), IconvError> {
    let bytes = input.get(..4).ok_or(IconvError::Incomplete)?;
    let array = [
        bytes.first().copied().unwrap_or(0),
        bytes.get(1).copied().unwrap_or(0),
        bytes.get(2).copied().unwrap_or(0),
        bytes.get(3).copied().unwrap_or(0),
    ];
    let code = if little {
        u32::from_le_bytes(array)
    } else {
        u32::from_be_bytes(array)
    };
    if (0xd800..0xe000).contains(&code) || code > 0x10ffff {
        return Err(IconvError::IllegalSequence);
    }
    Ok((code, 4))
}

fn encode_utf8(code: u32, out: &mut [u8; 8]) -> Result<(usize, usize), ()> {
    let mut buffer = [0u8; 4];
    let length = wcrtomb(MbMode::Utf8, code, &mut buffer).map_err(|_| ())?;
    if let Some(dst) = out.get_mut(..length)
        && let Some(src) = buffer.get(..length)
    {
        dst.copy_from_slice(src);
    }
    Ok((length, 0))
}

fn encode_utf16(
    code: u32,
    out: &mut [u8; 8],
    little: bool,
    ucs2: bool,
) -> Result<(usize, usize), ()> {
    let code = if ucs2 && code > 0xffff { 0xfffd } else { code };
    let put = |value: u16, at: usize, out: &mut [u8; 8]| {
        let bytes = if little {
            value.to_le_bytes()
        } else {
            value.to_be_bytes()
        };
        if let Some(dst) = out.get_mut(at..at + 2) {
            dst.copy_from_slice(&bytes);
        }
    };
    if code <= 0xffff {
        if (0xd800..0xe000).contains(&code) {
            return Err(());
        }
        put(code as u16, 0, out);
        return Ok((2, usize::from(ucs2 && code == 0xfffd)));
    }
    if ucs2 {
        put(0xfffd, 0, out);
        return Ok((2, 1));
    }
    if code > 0x10ffff {
        return Err(());
    }
    let value = code - 0x10000;
    put((0xd800 | (value >> 10)) as u16, 0, out);
    put((0xdc00 | (value & 0x3ff)) as u16, 2, out);
    Ok((4, 0))
}

fn encode_utf32(code: u32, out: &mut [u8; 8], little: bool) -> Result<(usize, usize), ()> {
    if (0xd800..0xe000).contains(&code) || code > 0x10ffff {
        return Err(());
    }
    let bytes = if little {
        code.to_le_bytes()
    } else {
        code.to_be_bytes()
    };
    if let Some(dst) = out.get_mut(..4) {
        dst.copy_from_slice(&bytes);
    }
    Ok((4, 0))
}

fn encode_legacy(offset: usize, code: u32, out: &mut [u8; 8]) -> Result<(usize, usize), ()> {
    if code < 128 {
        if let Some(slot) = out.get_mut(0) {
            *slot = code as u8;
        }
        return Ok((1, 0));
    }
    for byte in 0x80u16..=0xff {
        if legacy_map(offset, byte as u8) == code {
            if let Some(slot) = out.get_mut(0) {
                *slot = byte as u8;
            }
            return Ok((1, 0));
        }
    }
    Err(())
}

#[cfg(feature = "cjk")]
fn jis(row: u32, column: u32) -> Option<u32> {
    JIS0208
        .get(row as usize)
        .and_then(|line| line.get(column as usize))
        .copied()
        .filter(|value| *value != 0)
        .map(u32::from)
}

#[cfg(feature = "cjk")]
fn decode_euc_jp(input: &[u8]) -> Result<(u32, usize), IconvError> {
    let first = *input.first().ok_or(IconvError::Incomplete)?;
    if first < 0x80 {
        return Ok((u32::from(first), 1));
    }
    let bytes = input.get(..2).ok_or(IconvError::Incomplete)?;
    let second = u32::from(bytes.get(1).copied().unwrap_or(0));
    if first == 0x8e {
        if !(0xa1..=0xdf).contains(&second) {
            return Err(IconvError::IllegalSequence);
        }
        return Ok((second + 0xff61 - 0xa1, 2));
    }
    let row = u32::from(first).saturating_sub(0xa1);
    let column = second.saturating_sub(0xa1);
    let code = jis(row, column).ok_or(IconvError::IllegalSequence)?;
    Ok((code, 2))
}

#[cfg(feature = "cjk")]
fn decode_shift_jis(input: &[u8]) -> Result<(u32, usize), IconvError> {
    let first = *input.first().ok_or(IconvError::Incomplete)?;
    if first < 0x80 {
        return Ok((u32::from(first), 1));
    }
    let second = *input.get(1).ok_or(IconvError::Incomplete)?;
    if (0xa1..=0xdf).contains(&first) {
        return Ok((u32::from(first) + 0xff61 - 0xa1, 1));
    }
    let mut row = if (0x81..=0x9f).contains(&first) {
        u32::from(first) - 0x81
    } else if (0xe0..=0xef).contains(&first) {
        u32::from(first) - 0xc1
    } else {
        return Err(IconvError::IllegalSequence);
    };
    row = row.saturating_mul(2);
    let column = if (0x40..=0x9e).contains(&second) {
        if second == 0x7f {
            return Err(IconvError::IllegalSequence);
        }
        let mut value = u32::from(second);
        if value > 0x7f {
            value -= 1;
        }
        value.saturating_sub(0x40)
    } else if (0x9f..=0xfc).contains(&second) {
        row = row.saturating_add(1);
        u32::from(second).saturating_sub(0x9f)
    } else {
        return Err(IconvError::IllegalSequence);
    };
    let code = jis(row, column).ok_or(IconvError::IllegalSequence)?;
    Ok((code, 2))
}

#[cfg(feature = "cjk")]
fn decode_gb(kind: Cjk, input: &[u8]) -> Result<(u32, usize), IconvError> {
    let first = *input.first().ok_or(IconvError::Incomplete)?;
    if first < 0x80 {
        return Ok((u32::from(first), 1));
    }
    if first == 0x80 && kind == Cjk::Gbk {
        return Ok((0x20ac, 1));
    }
    let second = *input.get(1).ok_or(IconvError::Incomplete)?;
    if second == 0x80 && kind == Cjk::Gbk {
        return Ok((0x20ac, 1));
    }
    if second < 0xa1 && kind == Cjk::Gb2312 {
        return Err(IconvError::IllegalSequence);
    }
    if second.wrapping_sub(0x40) >= 191 || second == 0x7f {
        if !(0x30..=0x39).contains(&second) || kind != Cjk::Gb18030 {
            return Err(IconvError::IllegalSequence);
        }
        let third = *input.get(2).ok_or(IconvError::Incomplete)?;
        let fourth = *input.get(3).ok_or(IconvError::Incomplete)?;
        if !(0x81..=0xfe).contains(&third) || !(0x30..=0x39).contains(&fourth) {
            return Err(IconvError::IllegalSequence);
        }
        let row = u32::from(first).saturating_sub(0x81);
        let column = u32::from(second).saturating_sub(0x30);
        let mut code = (10 * row + column) * 1260;
        code = code.saturating_add(10 * (u32::from(third) - 0x81));
        code = code.saturating_add(u32::from(fourth) - 0x30);
        if code >= 189000 {
            code = code.saturating_sub(189000).saturating_add(0x10000);
            if code >= 0x110000 {
                return Err(IconvError::IllegalSequence);
            }
            return Ok((code, 4));
        }
        for pair in GB18030UTF {
            if code < u32::from(pair[1]) {
                return Ok((code + u32::from(pair[0]), 4));
            }
            code = code.saturating_sub(u32::from(pair[1]));
        }
        return Err(IconvError::IllegalSequence);
    }
    let row = u32::from(first).saturating_sub(0x81);
    let column = u32::from(second).saturating_sub(0x40);
    let column = if column > 63 { column - 1 } else { column };
    let code = GB18030_TABLE
        .get(row as usize)
        .and_then(|line| line.get(column as usize))
        .copied()
        .filter(|value| *value != 0)
        .ok_or(IconvError::IllegalSequence)?;
    Ok((u32::from(code), 2))
}

#[cfg(feature = "cjk")]
fn decode_big5(input: &[u8]) -> Result<(u32, usize), IconvError> {
    let first = *input.first().ok_or(IconvError::Incomplete)?;
    if first < 0x80 {
        return Ok((u32::from(first), 1));
    }
    let second = *input.get(1).ok_or(IconvError::Incomplete)?;
    if second.wrapping_sub(0x40) >= 0xbf || (0x7f..0xa1).contains(&second) {
        return Err(IconvError::IllegalSequence);
    }
    let column = if second > 0x7e {
        u32::from(second) - 0x62
    } else {
        u32::from(second) - 0x40
    };
    let row = u32::from(first).saturating_sub(0xa1);
    if row >= 0x59 {
        return Err(IconvError::IllegalSequence);
    }
    let code = BIG5_TABLE
        .get(row as usize)
        .and_then(|line| line.get(column as usize))
        .copied()
        .filter(|value| *value != 0)
        .ok_or(IconvError::IllegalSequence)?;
    Ok((u32::from(code), 2))
}

#[cfg(feature = "cjk")]
fn decode_euc_kr(input: &[u8]) -> Result<(u32, usize), IconvError> {
    let first = *input.first().ok_or(IconvError::Incomplete)?;
    if first < 0x80 {
        return Ok((u32::from(first), 1));
    }
    let second = *input.get(1).ok_or(IconvError::Incomplete)?;
    let row = u32::from(first).saturating_sub(0xa1);
    let column = u32::from(second).saturating_sub(0xa1);
    let code = KSC
        .get(row as usize)
        .and_then(|line| line.get(column as usize))
        .copied()
        .filter(|value| *value != 0)
        .ok_or(IconvError::IllegalSequence)?;
    Ok((u32::from(code), 2))
}

#[cfg(feature = "cjk")]
fn uni_to_jis(code: u32) -> Option<u32> {
    let mut base = 0usize;
    let mut count = REVJIS.len();
    loop {
        let middle = count / 2;
        let index = REVJIS.get(base.checked_add(middle)?).copied()?;
        let value = JIS0208
            .get(usize::from(index / 256))
            .and_then(|row| row.get(usize::from(index % 256)))
            .copied()
            .unwrap_or(0);
        if u32::from(value) == code {
            return Some(u32::from(index) + 0x2121);
        }
        if count == 1 {
            return None;
        }
        if code < u32::from(value) {
            count /= 2;
        } else {
            base = base.checked_add(middle)?;
            count -= count / 2;
        }
    }
}

#[cfg(feature = "cjk")]
fn encode_cjk(kind: Cjk, code: u32, out: &mut [u8; 8]) -> Result<(usize, usize), ()> {
    match kind {
        Cjk::EucJp | Cjk::ShiftJis | Cjk::Iso2022Jp => {
            if code < 128 {
                if let Some(slot) = out.get_mut(0) {
                    *slot = code as u8;
                }
                return Ok((1, 0));
            }
            let jis = uni_to_jis(code).ok_or(())?;
            let row = jis / 256;
            let column = jis % 256;
            match kind {
                Cjk::EucJp => {
                    if let Some(dst) = out.get_mut(..2) {
                        dst[0] = (row + 0x80) as u8;
                        dst[1] = (column + 0x80) as u8;
                    }
                    Ok((2, 0))
                }
                Cjk::ShiftJis => {
                    let lead = row.div_ceil(2) + if row < 95 { 112 } else { 176 };
                    let trail = if row % 2 == 1 {
                        column + 31 + column / 96
                    } else {
                        column + 126
                    };
                    if let Some(dst) = out.get_mut(..2) {
                        dst[0] = lead as u8;
                        dst[1] = trail as u8;
                    }
                    Ok((2, 0))
                }
                _ => {
                    if let Some(dst) = out.get_mut(..8) {
                        dst.copy_from_slice(&[
                            0x1b,
                            b'$',
                            b'B',
                            row as u8,
                            column as u8,
                            0x1b,
                            b'(',
                            b'B',
                        ]);
                    }
                    Ok((8, 0))
                }
            }
        }
        Cjk::Gb18030 | Cjk::Gbk | Cjk::Gb2312 | Cjk::Big5 | Cjk::EucKr => Err(()),
    }
}

#[cfg(not(feature = "cjk"))]
fn encode_cjk(_kind: Cjk, _code: u32, _out: &mut [u8; 8]) -> Result<(usize, usize), ()> {
    Err(())
}

fn from_type_from(kind: u8, offset: usize) -> Charset {
    if is_legacy(kind) {
        Charset::Legacy(offset)
    } else {
        from_type(kind, offset)
    }
}

#[cfg(test)]
#[path = "convert_ut.rs"]
mod tests;
