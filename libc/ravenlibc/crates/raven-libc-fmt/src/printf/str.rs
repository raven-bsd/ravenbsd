// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `%c`, `%s`, `%lc`, `%ls`, `%C` and `%S` (CLEANROOM).
//!
//! The string rules are C11 7.21.6.1p8 and 7.29.2.1, read through the FreeBSD `__wcsconv`
//! behaviour: narrow precision counts output bytes and never splits a UTF-8 sequence; wide
//! precision counts output characters; a NULL pointer prints the six characters `(null)`
//! cut by the precision; invalid multibyte input fails with [`FmtErr::Encoding`]. No donor
//! file is copied here; the routines read the string only through [`FmtEnv`].

use raven_libc_ctype::{MbMode, MbState, Mbr, mbrtowc, wcrtomb};

use crate::traits::{CharUnit, FmtEnv, Sink};
use crate::types::FmtErr;

/// The literal printed for a NULL `%s`/`%ls`.
pub(crate) const NULL_STRING: &[u8] = b"(null)";

/// The narrow `%s` length: `min(strlen, prec)` bytes.
pub(crate) fn cstr_size<A: FmtEnv>(args: &mut A, ptr: usize, prec: i32) -> usize {
    let max = if prec < 0 { usize::MAX } else { prec as usize };
    args.cstr_len(ptr, max)
}

/// Emit exactly `len` bytes of a narrow string as output units.
///
/// The byte values are passed through unchanged; this is only used for narrow output.
pub(crate) fn emit_cstr<C: CharUnit, S: Sink<C>, A: FmtEnv>(
    sink: &mut S,
    args: &mut A,
    ptr: usize,
    len: usize,
) -> Result<(), FmtErr> {
    let mut off = 0usize;
    let mut buf = [0u8; 256];
    while off < len {
        let want = core::cmp::min(buf.len(), len.saturating_sub(off));
        let n = args.read_bytes(ptr, off, buf.get_mut(..want).unwrap_or(&mut []));
        if n == 0 {
            break;
        }
        for &b in buf.get(..n).unwrap_or(&[]) {
            sink.write(&[C::from_ascii(b)]).map_err(|_| FmtErr::Sink)?;
        }
        off = off.saturating_add(n);
    }
    Ok(())
}

/// The narrow `%ls` length: encoded bytes, stopping before a sequence that would exceed
/// `prec` bytes (the caller passes `prec < 0` for no limit).
///
/// # Errors
/// [`FmtErr::Encoding`] for an invalid wide character or an incomplete multibyte state.
pub(crate) fn wstr_mb_size<A: FmtEnv>(
    args: &mut A,
    ptr: usize,
    prec: i32,
    mode: MbMode,
) -> Result<usize, FmtErr> {
    let max = if prec < 0 { usize::MAX } else { prec as usize };
    let mut total = 0usize;
    let mut off = 0usize;
    let mut buf = [0u32; 128];
    loop {
        let n = args.read_wchars(ptr, off, &mut buf);
        if n == 0 {
            break;
        }
        for &wc in buf.get(..n).unwrap_or(&[]) {
            if wc == 0 {
                return Ok(total);
            }
            let mut mb = [0u8; 4];
            let k = wcrtomb(mode, wc, &mut mb).map_err(|_| FmtErr::Encoding)?;
            if total.saturating_add(k) > max {
                return Ok(total);
            }
            total = total.saturating_add(k);
            off = off.saturating_add(1);
        }
    }
    Ok(total)
}

/// Emit a narrow `%ls` conversion, at most `limit` output bytes.
///
/// # Errors
/// [`FmtErr::Encoding`] for invalid wide input.
pub(crate) fn emit_wstr_mb<C: CharUnit, S: Sink<C>, A: FmtEnv>(
    sink: &mut S,
    args: &mut A,
    ptr: usize,
    limit: usize,
    mode: MbMode,
) -> Result<(), FmtErr> {
    let mut total = 0usize;
    let mut off = 0usize;
    let mut buf = [0u32; 128];
    'outer: loop {
        let n = args.read_wchars(ptr, off, &mut buf);
        if n == 0 {
            break;
        }
        for &wc in buf.get(..n).unwrap_or(&[]) {
            if wc == 0 {
                break 'outer;
            }
            let mut mb = [0u8; 4];
            let k = wcrtomb(mode, wc, &mut mb).map_err(|_| FmtErr::Encoding)?;
            if total.saturating_add(k) > limit {
                break 'outer;
            }
            for &b in mb.get(..k).unwrap_or(&[]) {
                sink.write(&[C::from_ascii(b)]).map_err(|_| FmtErr::Sink)?;
            }
            total = total.saturating_add(k);
            off = off.saturating_add(1);
        }
    }
    Ok(())
}

/// The wide `%ls` length in characters (`wstr_len`, cut by `prec` characters).
pub(crate) fn wstr_size<A: FmtEnv>(args: &mut A, ptr: usize, prec: i32) -> usize {
    let max = if prec < 0 { usize::MAX } else { prec as usize };
    args.wstr_len(ptr, max)
}

/// Emit exactly `len` wide characters.
pub(crate) fn emit_wstr<C: CharUnit, S: Sink<C>, A: FmtEnv>(
    sink: &mut S,
    args: &mut A,
    ptr: usize,
    len: usize,
) -> Result<(), FmtErr> {
    let mut off = 0usize;
    let mut buf = [0u32; 128];
    while off < len {
        let want = core::cmp::min(buf.len(), len.saturating_sub(off));
        let n = args.read_wchars(ptr, off, buf.get_mut(..want).unwrap_or(&mut []));
        if n == 0 {
            break;
        }
        for &wc in buf.get(..n).unwrap_or(&[]) {
            sink.write(&[C::from_u32(wc)]).map_err(|_| FmtErr::Sink)?;
        }
        off = off.saturating_add(n);
    }
    Ok(())
}

/// Stream `total` bytes of multibyte input through a decoder, one character at a time.
///
/// The closure returns `false` to stop before the next character.
fn decode_mb<A: FmtEnv>(
    args: &mut A,
    ptr: usize,
    total: usize,
    mode: MbMode,
    mut emit: impl FnMut(u32) -> Result<bool, FmtErr>,
) -> Result<(), FmtErr> {
    let mut state = MbState::INITIAL;
    let mut carry = [0u8; 4];
    let mut carry_len = 0usize;
    let mut off = 0usize;
    let mut buf = [0u8; 260];
    loop {
        let mut n = carry_len;
        if let Some(head) = buf.get_mut(..carry_len) {
            head.copy_from_slice(carry.get(..carry_len).unwrap_or(&[]));
        }
        if off < total {
            let want = core::cmp::min(buf.len().saturating_sub(n), total.saturating_sub(off));
            let got = args.read_bytes(
                ptr,
                off,
                buf.get_mut(n..n.saturating_add(want)).unwrap_or(&mut []),
            );
            off = off.saturating_add(got);
            n = n.saturating_add(got);
        }
        if n == 0 {
            break;
        }
        let mut i = 0usize;
        let mut incomplete = 0usize;
        while i < n {
            match mbrtowc(&mut state, mode, buf.get(i..n).unwrap_or(&[])) {
                Mbr::Char { wc, used } => {
                    if !emit(wc)? {
                        return Ok(());
                    }
                    i = i.saturating_add(used.max(1));
                }
                Mbr::Incomplete => {
                    incomplete = n.saturating_sub(i);
                    break;
                }
                Mbr::Ilseq => return Err(FmtErr::Encoding),
            }
        }
        if incomplete > 0 {
            if incomplete > carry.len() {
                return Err(FmtErr::Encoding);
            }
            carry = [0u8; 4];
            if let Some(dst) = carry.get_mut(..incomplete) {
                dst.copy_from_slice(buf.get(i..n).unwrap_or(&[]));
            }
            carry_len = incomplete;
            if off >= total {
                return Err(FmtErr::Encoding);
            }
        } else {
            carry_len = 0;
        }
    }
    Ok(())
}

/// The wide `%s` length in output characters: decode the multibyte string and stop at
/// `prec` characters.
///
/// # Errors
/// [`FmtErr::Encoding`] for invalid multibyte input.
pub(crate) fn mb_wstr_size<A: FmtEnv>(
    args: &mut A,
    ptr: usize,
    prec: i32,
    mode: MbMode,
) -> Result<usize, FmtErr> {
    let total = args.cstr_len(ptr, usize::MAX);
    let max = if prec < 0 { usize::MAX } else { prec as usize };
    let mut count = 0usize;
    decode_mb(args, ptr, total, mode, |_wc| {
        if count >= max {
            return Ok(false);
        }
        count = count.saturating_add(1);
        Ok(true)
    })?;
    Ok(count)
}

/// Emit a wide `%s`: decode the multibyte string and write at most `limit` characters.
///
/// # Errors
/// [`FmtErr::Encoding`] for invalid multibyte input.
pub(crate) fn emit_mb_wstr<C: CharUnit, S: Sink<C>, A: FmtEnv>(
    sink: &mut S,
    args: &mut A,
    ptr: usize,
    limit: usize,
    mode: MbMode,
) -> Result<(), FmtErr> {
    let total = args.cstr_len(ptr, usize::MAX);
    let mut count = 0usize;
    decode_mb(args, ptr, total, mode, |wc| {
        if count >= limit {
            return Ok(false);
        }
        sink.write(&[C::from_u32(wc)]).map_err(|_| FmtErr::Sink)?;
        count = count.saturating_add(1);
        Ok(true)
    })
}

/// Compute the output length of one `%lc`/`%C` in narrow output.
///
/// # Errors
/// [`FmtErr::Encoding`] when the wide character has no multibyte encoding.
pub(crate) fn wchar_size(wc: u32, mode: MbMode) -> Result<usize, FmtErr> {
    let mut mb = [0u8; 4];
    wcrtomb(mode, wc, &mut mb).map_err(|_| FmtErr::Encoding)
}

/// Emit one `%lc`/`%C` in narrow output; returns the number of bytes written.
///
/// # Errors
/// [`FmtErr::Encoding`] or [`FmtErr::Sink`].
#[cfg(test)]
pub(crate) fn emit_wchar<C: CharUnit, S: Sink<C>>(
    sink: &mut S,
    wc: u32,
    mode: MbMode,
) -> Result<usize, FmtErr> {
    let mut mb = [0u8; 4];
    let k = wcrtomb(mode, wc, &mut mb).map_err(|_| FmtErr::Encoding)?;
    for &b in mb.get(..k).unwrap_or(&[]) {
        sink.write(&[C::from_ascii(b)]).map_err(|_| FmtErr::Sink)?;
    }
    Ok(k)
}

#[cfg(test)]
#[allow(
    clippy::all,
    clippy::pedantic,
    clippy::nursery,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects
)]
#[path = "str_ut.rs"]
mod tests;
