// SPDX-License-Identifier: MIT
// Copyright © 2005-2020 Rich Felker, et al.
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// RavenBSD: transpiled from musl:src/stdio/fgetwc.c @ 7813e9119eb5fe681b6750b8a033a0490bca0941
// The donor file has no per-file header; it is covered by musl/COPYRIGHT (lines 1-24).
// MIT via musl/COPYRIGHT.
//
// Permission is hereby granted, free of charge, to any person obtaining
// a copy of this software and associated documentation files (the
// "Software"), to deal in the Software without restriction, including
// without limitation the rights to use, copy, modify, merge, publish,
// distribute, sublicense, and/or sell copies of the Software, and to
// permit persons to whom the Software is furnished to do so, subject to
// the following conditions:
//
// The above copyright notice and this permission notice shall be
// included in all copies or substantial portions of the Software.
//
// THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND,
// EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF
// MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT.
// IN NO EVENT SHALL THE AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY
// CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER IN AN ACTION OF CONTRACT,
// TORT OR OTHERWISE, ARISING FROM, OUT OF OR IN CONNECTION WITH THE
// SOFTWARE OR THE USE OR OTHER DEALINGS IN THE SOFTWARE.
//
//! Wide-oriented stdio (`fgetwc.c`, `fputwc.c`, `ungetwc.c`, `fgetws.c`, `fputws.c`,
//! `getwc.c`, `putwc.c`, `getwchar.c`, `putwchar.c`, `fwide.c`).
//!
//! Orientation is the `mode` field: 0 unset, -1 byte, 1 wide. The first wide operation sets
//! it; `fwide(f, 0)` only queries and `fwide(f, n)` is a no-op once the stream is oriented
//! (T16). Decoding and encoding go through `raven-libc-ctype::{mbrtowc, wcrtomb}` with the
//! per-FILE [`MbState`] and the byte/UTF-8 mode the stream carries (H-3); `ungetwc` pushes the
//! UTF-8 encoding back into the [`crate::file::UNGET`] area (up to four bytes), and
//! `open_wmemstream` is born wide.
//!
//! Mixing orientations fails: a byte operation on a wide stream, or a wide operation on a
//! byte stream, returns `EOF`/`WEOF` and sets `F_ERR`. musl's current code does not test the
//! byte case; the area document requires the failure (declared deviation, oracle exclusion).
#![allow(
    clippy::arithmetic_side_effects,
    clippy::indexing_slicing,
    clippy::collapsible_if
)]
// RavenBSD: the donor's pointer arithmetic, branch nesting and bounded indices are preserved;
// every index is checked against the region or the caller's slice before use (`file.rs`
// invariants), and the arithmetic is the C `size_t`/`pointer` arithmetic of the donor.

// The pushback and encode paths are index arithmetic over the fixed region.

use raven_libc_abi::generated::errno::{EBADF, EILSEQ};
use raven_libc_ctype::{MbState, Mbr, mbrtowc, wcrtomb};

use crate::file::{F_EOF, F_ERR, File};
use crate::lock::FileGuard;
use crate::state::{State, StdKind};
use crate::traits::Env;
use crate::{Fail, R};

/// `WEOF` in the `wint_t` domain.
pub const WEOF: u32 = u32::MAX;

/// `fwide`: query or set the orientation.
pub fn fwide<E: Env>(file: &File<E>, mode: i32) -> i32 {
    let _guard = FileGuard::new(file);
    if mode != 0 {
        let st = file.st_mut();
        if st.mode == 0 {
            st.mode = if mode > 0 { 1 } else { -1 };
        }
    }
    i32::from(file.st().mode)
}

/// Set the wide orientation for an operation that is about to take the stream.
fn orient_wide<E: Env>(file: &File<E>) -> R<()> {
    if file.st().mode == -1 {
        file.st_mut().flags |= F_ERR;
        return Err(Fail::Eof);
    }
    if file.st().mode == 0 {
        let _ = fwide(file, 1);
    }
    Ok(())
}

/// Encode one unit into the UTF-8 buffer.
fn encode<E: Env>(file: &File<E>, wc: u32) -> R<(usize, [u8; 4])> {
    let mut bytes = [0u8; 4];
    match wcrtomb(file.st().mbmode, wc, &mut bytes) {
        Ok(len) => Ok((len, bytes)),
        Err(_) => {
            file.st_mut().flags |= F_ERR;
            Err(Fail::from_int(EILSEQ))
        }
    }
}

/// `__fgetwc_unlocked`: decode one wide character.
pub(crate) fn fgetwc_unlocked<E: Env>(file: &File<E>) -> R<u32> {
    orient_wide(file)?;
    if file.st().rpos != file.st().rend {
        let mode = file.st().mbmode;
        let decoded = {
            let st = file.st_mut();
            let window = st.buffer.bytes().get(st.rpos..st.rend).unwrap_or(&[]);
            mbrtowc(&mut st.mbstate, mode, window)
        };
        match decoded {
            Mbr::Char { wc, used } => {
                file.st_mut().rpos += used;
                return Ok(wc);
            }
            Mbr::Incomplete | Mbr::Ilseq => {
                file.st_mut().mbstate = MbState::INITIAL;
            }
        }
    }
    let mode = file.st().mbmode;
    let mut state = MbState::INITIAL;
    let mut first = true;
    loop {
        let c = match crate::char_io::getc_unlocked(file) {
            Ok(c) => c,
            Err(error) => {
                if !first {
                    file.st_mut().flags |= F_ERR;
                }
                return match error {
                    Fail::Eof => Err(Fail::Eof),
                    Fail::Errno(code) => Err(Fail::Errno(code)),
                };
            }
        };
        let byte = c as u8;
        match mbrtowc(&mut state, mode, &[byte]) {
            Mbr::Char { wc, .. } => return Ok(wc),
            Mbr::Incomplete => first = false,
            Mbr::Ilseq => {
                file.st_mut().flags |= F_ERR;
                if !first {
                    let _ = crate::ungetc::ungetc(file, i32::from(byte));
                }
                return Err(Fail::Eof);
            }
        }
    }
}

/// `fgetwc`.
///
/// # Errors
/// [`Fail::Eof`] at end of input, on an invalid sequence (`EILSEQ` and `F_ERR`), or when the
/// stream is byte oriented.
pub fn fgetwc<E: Env>(file: &File<E>) -> R<u32> {
    let _guard = FileGuard::new(file);
    fgetwc_unlocked(file)
}

/// `__fputwc_unlocked`.
pub(crate) fn fputwc_unlocked<E: Env>(file: &File<E>, wc: u32) -> R<u32> {
    orient_wide(file)?;
    if wc < 0x80 {
        let _ = crate::char_io::putc_unlocked(file, wc as i32)?;
        return Ok(wc);
    }
    let (len, bytes) = encode(file, wc)?;
    let space = {
        let st = file.st();
        if st.wpos != 0 {
            st.wend.saturating_sub(st.wpos)
        } else {
            0
        }
    };
    if space > 4 {
        let st = file.st_mut();
        let region = st.buffer.bytes_mut();
        if let Some(slot) = region.get_mut(st.wpos..st.wpos + len) {
            slot.copy_from_slice(&bytes[..len]);
            st.wpos += len;
            return Ok(wc);
        }
    }
    match crate::block_io::fwritex(file, &bytes[..len]) {
        Ok(count) if count == len => Ok(wc),
        Ok(_) => {
            file.st_mut().flags |= F_ERR;
            Err(Fail::Eof)
        }
        Err(error) => {
            file.st_mut().flags |= F_ERR;
            Err(error)
        }
    }
}

/// `fputwc`.
///
/// # Errors
/// [`Fail::Errno`] with `EILSEQ` for an unencodable unit, [`Fail::Eof`] on a write error or a
/// byte-oriented stream.
pub fn fputwc<E: Env>(file: &File<E>, wc: u32) -> R<u32> {
    let _guard = FileGuard::new(file);
    fputwc_unlocked(file, wc)
}

/// `getwc`.
pub fn getwc<E: Env>(file: &File<E>) -> R<u32> {
    fgetwc(file)
}

/// `putwc`.
pub fn putwc<E: Env>(file: &File<E>, wc: u32) -> R<u32> {
    fputwc(file, wc)
}

/// `getwchar`.
pub fn getwchar<E: Env>(state: &'static State<E>) -> R<u32> {
    let Some(file) = state.std_file(StdKind::In) else {
        return Err(Fail::from_int(EBADF));
    };
    fgetwc(file)
}

/// `putwchar`.
pub fn putwchar<E: Env>(state: &'static State<E>, wc: u32) -> R<u32> {
    let Some(file) = state.std_file(StdKind::Out) else {
        return Err(Fail::from_int(EBADF));
    };
    fputwc(file, wc)
}

/// `ungetwc`.
///
/// # Errors
/// [`Fail::Eof`] for `WEOF`, a byte-oriented stream, an unencodable unit or an exhausted
/// pushback area.
pub fn ungetwc<E: Env>(file: &File<E>, wc: u32) -> R<u32> {
    if wc == WEOF {
        return Err(Fail::Eof);
    }
    let _guard = FileGuard::new(file);
    orient_wide(file)?;
    if file.st().rpos == 0 {
        let _ = crate::mode::toread(file);
    }
    if file.st().rpos == 0 {
        return Err(Fail::Eof);
    }
    let (len, bytes) = encode(file, wc)?;
    if file.st().rpos < len {
        return Err(Fail::Eof);
    }
    let rpos = file.st().rpos;
    {
        let st = file.st_mut();
        let region = st.buffer.bytes_mut();
        let start = rpos - len;
        if let Some(slot) = region.get_mut(start..rpos) {
            slot.copy_from_slice(&bytes[..len]);
        }
        st.rpos = start;
        st.flags &= !F_EOF;
    }
    Ok(wc)
}

/// `fgetws`: read wide characters up to a newline or `dest.len() - 1`.
///
/// Returns the number of units stored, excluding the NUL.
///
/// # Errors
/// [`Fail::Eof`] at end of input with nothing read or on a stream error.
pub fn fgetws<E: Env>(file: &File<E>, dest: &mut [u32]) -> R<usize> {
    let _guard = FileGuard::new(file);
    if dest.is_empty() {
        return Ok(0);
    }
    let limit = dest.len() - 1;
    let mut stored = 0usize;
    while stored < limit {
        match fgetwc_unlocked(file) {
            Ok(wc) => {
                dest[stored] = wc;
                stored += 1;
                if wc == u32::from(b'\n') {
                    break;
                }
            }
            Err(Fail::Eof) => {
                if file.st().flags & F_ERR != 0 {
                    dest[0] = 0;
                    return Err(Fail::Eof);
                }
                break;
            }
            Err(error) => {
                dest[0] = 0;
                return Err(error);
            }
        }
    }
    dest[stored] = 0;
    if stored == 0 && file.st().flags & F_ERR != 0 {
        return Err(Fail::Eof);
    }
    if stored == 0 && file.st().flags & F_EOF != 0 {
        return Err(Fail::Eof);
    }
    Ok(stored)
}

/// `fputws`: write a NUL-terminated wide string as UTF-8.
///
/// # Errors
/// [`Fail::Eof`] on a short write or a stream error.
pub fn fputws<E: Env>(file: &File<E>, text: &[u32]) -> R<()> {
    let _guard = FileGuard::new(file);
    let _ = fwide(file, 1);
    let mut buffer = [0u8; 1024];
    let mut used = 0usize;
    for &wc in text {
        if wc == 0 {
            break;
        }
        let (len, bytes) = encode(file, wc)?;
        if used + len > buffer.len() {
            if used != 0 {
                let written = crate::block_io::fwritex(file, &buffer[..used])?;
                if written != used {
                    return Err(Fail::Eof);
                }
                used = 0;
            }
        }
        if used + len <= buffer.len() {
            buffer[used..used + len].copy_from_slice(&bytes[..len]);
            used += len;
        } else {
            return Err(Fail::Eof);
        }
    }
    if used != 0 {
        let written = crate::block_io::fwritex(file, &buffer[..used])?;
        if written != used {
            return Err(Fail::Eof);
        }
    }
    Ok(())
}

#[cfg(test)]
#[path = "wide_ut.rs"]
mod tests;
