// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 1990, 1993
//	The Regents of the University of California.  All rights reserved.
//
// This code is derived from software contributed to Berkeley by
// Chris Torek.
//
// Copyright (c) 2011 The FreeBSD Foundation
// Portions of this software were developed by David Chisnall
// under sponsorship from the FreeBSD Foundation.
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// RavenBSD: transpiled from freebsd:lib/libc/stdio/vfprintf.c @ 133344405f532bd54ff3629a77d4c4e062f01fdb
// (__vfprintf, line 302; grouping_print line 122; __wcsconv line 211 is replaced by the
// CLEANROOM string module)
//
// Redistribution and use in source and binary forms, with or without
// modification, are permitted provided that the following conditions
// are met:
// 1. Redistributions of source code must retain the above copyright
//    notice, this list of conditions and the following disclaimer.
// 2. Redistributions in binary form must reproduce the above copyright
//    notice, this list of conditions and the following disclaimer in the
//    documentation and/or other materials provided with the distribution.
// 3. Neither the name of the University nor the names of its contributors
//    may be used to endorse or promote products derived from this software
//    without specific prior written permission.
//
// THIS SOFTWARE IS PROVIDED BY THE REGENTS AND CONTRIBUTORS ``AS IS'' AND
// ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT LIMITED TO, THE
// IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE
// ARE DISCLAIMED.  IN NO EVENT SHALL THE REGENTS OR CONTRIBUTORS BE LIABLE
// FOR ANY DIRECT, INDIRECT, INCIDENTAL, SPECIAL, EXEMPLARY, OR CONSEQUENTIAL
// DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS
// OR SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION)
// HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY, WHETHER IN CONTRACT, STRICT
// LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY
// OUT OF THE USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF
// SUCH DAMAGE.
//
//! The `printf` conversion loop, `vformat` and `vformat_dry`.
//!
//! The `io_state`/`__suio` layer of the donor is replaced by [`Sink`] calls (`PRINT` is
//! `sink.write`, `PAD` is `sink.pad`, `FLUSH` is nothing); `__sbprintf`, `xprintf` and the
//! `register_printf_*` path are deleted. `locale_t` became [`crate::NumericFormat`],
//! `va_arg` became [`crate::VaReader`] and `errno = EOVERFLOW` became [`FmtErr::Overflow`].
//!
//! Every conversion that consumes an argument goes through `ArgReader`, which either
//! draws from the positional table or advances the reader; the `*` width and precision use
//! the same path. The common emission preserves the donor's order: blank padding, sign,
//! `0x` prefix, zero padding, precision zeros, digits/string, left padding.

#![allow(clippy::arithmetic_side_effects, clippy::indexing_slicing)]
// The main loop is a statement-for-statement port of `__vfprintf`; each output-size
// addition carries its explicit INT_MAX check, and every format read goes through
// `fmt.get`.

pub(crate) mod float_layout;
pub(crate) mod group;
pub(crate) mod int;
pub(crate) mod pos;
pub(crate) mod spec;
pub(crate) mod str;

use float_layout::FloatConv;
use group::Grouping;
use spec::{
    ALT, ArgTag, ArgVal, CHARINT, GROUPING, INTMAX_SIZE, INTMAXT, LADJUST, LLONGINT, LONGDBL,
    LONGINT, PTRDIFFT, SHORTINT, SIZET, ZEROPAD,
};

use crate::traits::{CharUnit, FloatFmt, FmtEnv, Sink};
use crate::types::{FloatBits, FloatKind, FmtErr, FmtOptions, PctN, StoreSize};

const INT_MAX: i64 = i32::MAX as i64;

/// One argument source: the positional table or the sequential reader.
struct ArgReader<'a, A: FmtEnv> {
    args: &'a mut A,
    table: Option<&'a (
        [ArgVal; crate::types::NL_ARGMAX + 1],
        [ArgTag; crate::types::NL_ARGMAX + 1],
    )>,
    next: u32,
}

impl<A: FmtEnv> ArgReader<'_, A> {
    fn index(&mut self, pos: Option<u32>) -> Result<usize, FmtErr> {
        if let Some(table) = self.table {
            let n = pos.ok_or(FmtErr::Invalid)?;
            if n == 0 || n as usize >= table.0.len() {
                return Err(FmtErr::Invalid);
            }
            Ok(n as usize)
        } else {
            if pos.is_some() {
                return Err(FmtErr::Invalid);
            }
            let n = self.next;
            self.next = self.next.saturating_add(1);
            Ok(n as usize)
        }
    }

    fn word(&mut self, pos: Option<u32>) -> Result<u64, FmtErr> {
        let index = self.index(pos)?;
        if let Some(table) = self.table {
            let value = table.0.get(index).copied().unwrap_or(ArgVal::word(0));
            Ok(value.a)
        } else {
            Ok(self.args.next_u64())
        }
    }

    fn f64_bits(&mut self, pos: Option<u32>) -> Result<u64, FmtErr> {
        let index = self.index(pos)?;
        if let Some(table) = self.table {
            let tag = table.1.get(index).copied().unwrap_or(ArgTag::Int);
            if tag == ArgTag::F64 || tag == ArgTag::Int {
                Ok(table.0.get(index).copied().unwrap_or(ArgVal::word(0)).a)
            } else {
                Err(FmtErr::Invalid)
            }
        } else {
            Ok(self.args.next_f64_bits())
        }
    }

    fn ld_bits(&mut self, pos: Option<u32>) -> Result<u128, FmtErr> {
        let index = self.index(pos)?;
        if let Some(table) = self.table {
            let tag = table.1.get(index).copied().unwrap_or(ArgTag::Int);
            if tag == ArgTag::Ld {
                Ok(table.0.get(index).copied().unwrap_or(ArgVal::word(0)).ld())
            } else {
                Err(FmtErr::Invalid)
            }
        } else {
            Ok(self.args.next_ld_bits())
        }
    }
}

/// The body of one conversion, ready for the common emission order.
enum Body<'a> {
    /// ASCII bytes, widened one unit at a time.
    Bytes(&'a [u8]),
    /// Explicit output units.
    Units(&'a [u32]),
    /// Integer digits, optionally grouped.
    Digits {
        /// The raw digits.
        digits: &'a [u8],
        /// The grouping state, already initialised for `digits.len()`.
        grouping: Option<Grouping<'a>>,
    },
    /// A narrow string read from memory.
    NarrowStr {
        /// The address.
        ptr: usize,
        /// The byte count.
        len: usize,
    },
    /// A wide string converted to multibyte output.
    WStrMb {
        /// The address.
        ptr: usize,
        /// The output byte limit.
        len: usize,
    },
    /// A wide string copied to wide output.
    WStr {
        /// The address.
        ptr: usize,
        /// The character count.
        len: usize,
    },
    /// A multibyte string decoded to wide output.
    MbStr {
        /// The address.
        ptr: usize,
        /// The character count.
        len: usize,
    },
}

fn digit(ch: u32) -> bool {
    (0x30..=0x39).contains(&ch)
}

fn to_digit(ch: u32) -> u32 {
    ch.wrapping_sub(0x30)
}

/// Parse `m$` after `*`; `i` points just after the `*`.
fn star_index<C: CharUnit>(fmt: &[C], start: usize) -> (Option<u32>, usize) {
    let mut i = start;
    let mut n = 0u32;
    while let Some(&c) = fmt.get(i) {
        let v = c.to_u32();
        if !digit(v) {
            break;
        }
        n = n.saturating_mul(10).saturating_add(to_digit(v));
        i = i.wrapping_add(1);
    }
    if fmt.get(i).map(|c| c.to_u32()) == Some(u32::from(b'$')) {
        (Some(n), i.wrapping_add(1))
    } else {
        (None, start)
    }
}

/// Format `fmt` into `sink` with the arguments read from `args`.
///
/// # Errors
/// See [`FmtErr`]. A positional-argument error is detected before the first write; a sink
/// error is reported as [`FmtErr::Sink`]; a result above `i32::MAX` is
/// [`FmtErr::Overflow`].
pub fn vformat<C: CharUnit, S: Sink<C>, A: FmtEnv, F: FloatFmt>(
    sink: &mut S,
    fmt: &[C],
    args: &mut A,
    opts: &FmtOptions<'_>,
    floats: &F,
) -> Result<i32, FmtErr> {
    let analysis = pos::analyse(fmt)?;
    let values = match analysis.table {
        Some(table) => Some(pos::fetch(&table, &mut args.clone_state())?),
        None => None,
    };
    let mut reader = ArgReader {
        args,
        table: values.as_ref(),
        next: 1,
    };
    let mut ret: i64 = 0;
    let mut i = 0usize;
    'outer: loop {
        let start = i;
        while let Some(&c) = fmt.get(i) {
            let v = c.to_u32();
            if v == 0 || v == u32::from(b'%') {
                break;
            }
            i = i.wrapping_add(1);
        }
        let literal = fmt.get(start..i).unwrap_or(&[]);
        if !literal.is_empty() {
            if ret.saturating_add(literal.len() as i64) > INT_MAX {
                return Err(FmtErr::Overflow);
            }
            sink.write(literal).map_err(|_| FmtErr::Sink)?;
            ret = ret.saturating_add(literal.len() as i64);
        }
        if i >= fmt.len() || fmt.get(i).map(|c| c.to_u32()) == Some(0) {
            break;
        }
        i = i.wrapping_add(1); // skip '%'

        let mut flags = 0u32;
        let mut width = 0i32;
        let mut prec = -1i32;
        let mut sign = 0u8;
        let mut ox = 0u8;
        let mut argpos: Option<u32> = None;
        let mut ch = {
            let v = fmt.get(i).copied().unwrap_or_default().to_u32();
            i = i.wrapping_add(1);
            v
        };
        loop {
            match ch {
                0x20 => {
                    if sign == 0 {
                        sign = b' ';
                    }
                }
                0x23 => flags |= ALT,
                0x2d => flags |= LADJUST,
                0x2b => sign = b'+',
                0x27 => flags |= GROUPING,
                0x30 => flags |= ZEROPAD,
                0x2a => {
                    let (idx, next) = star_index(fmt, i);
                    i = next;
                    let v = reader.word(idx)? as i32;
                    if v >= 0 {
                        width = v;
                    } else {
                        width = v.wrapping_neg();
                        flags |= LADJUST;
                    }
                    ch = next_char(fmt, &mut i);
                    continue;
                }
                0x2e => {
                    ch = next_char(fmt, &mut i);
                    if ch == 0x2a {
                        let (idx, next) = star_index(fmt, i);
                        i = next;
                        prec = reader.word(idx)? as i32;
                        ch = next_char(fmt, &mut i);
                    } else {
                        let mut p = 0i32;
                        while digit(ch) {
                            p = p.saturating_mul(10).saturating_add(to_digit(ch) as i32);
                            ch = next_char(fmt, &mut i);
                        }
                        prec = p;
                    }
                    continue;
                }
                0x31..=0x39 => {
                    let mut n = 0u32;
                    let mut d = ch;
                    loop {
                        n = n.saturating_mul(10).saturating_add(to_digit(d));
                        d = next_char(fmt, &mut i);
                        if !digit(d) {
                            break;
                        }
                    }
                    if d == u32::from(b'$') {
                        argpos = Some(n);
                        ch = next_char(fmt, &mut i);
                    } else {
                        width = n as i32;
                        ch = d;
                    }
                    continue;
                }
                x if x == u32::from(b'L') => {
                    flags |= LONGDBL;
                    ch = next_char(fmt, &mut i);
                    continue;
                }
                x if x == u32::from(b'h') => {
                    if flags & SHORTINT != 0 {
                        flags &= !SHORTINT;
                        flags |= CHARINT;
                    } else {
                        flags |= SHORTINT;
                    }
                    ch = next_char(fmt, &mut i);
                    continue;
                }
                x if x == u32::from(b'j') => {
                    flags |= INTMAXT;
                    ch = next_char(fmt, &mut i);
                    continue;
                }
                x if x == u32::from(b'l') => {
                    if flags & LONGINT != 0 {
                        flags &= !LONGINT;
                        flags |= LLONGINT;
                    } else {
                        flags |= LONGINT;
                    }
                    ch = next_char(fmt, &mut i);
                    continue;
                }
                x if x == u32::from(b'q') => {
                    flags |= LLONGINT;
                    ch = next_char(fmt, &mut i);
                    continue;
                }
                x if x == u32::from(b't') => {
                    flags |= PTRDIFFT;
                    ch = next_char(fmt, &mut i);
                    continue;
                }
                x if x == u32::from(b'z') => {
                    flags |= SIZET;
                    ch = next_char(fmt, &mut i);
                    continue;
                }
                x if x == u32::from(b'w') => {
                    flags &= !(CHARINT | SHORTINT | LONGINT | LLONGINT | INTMAXT);
                    let fast = fmt.get(i).map(|c| c.to_u32()) == Some(u32::from(b'f'));
                    if fast {
                        flags |= spec::FASTINT;
                        i = i.wrapping_add(1);
                    } else {
                        flags &= !spec::FASTINT;
                    }
                    let a = fmt.get(i).map(|c| c.to_u32());
                    let b = fmt.get(i.wrapping_add(1)).map(|c| c.to_u32());
                    if a == Some(u32::from(b'8')) {
                        if !fast {
                            flags |= CHARINT;
                        }
                        i = i.wrapping_add(1);
                    } else if a == Some(u32::from(b'1')) && b == Some(u32::from(b'6')) {
                        if !fast {
                            flags |= SHORTINT;
                        }
                        i = i.wrapping_add(2);
                    } else if a == Some(u32::from(b'3')) && b == Some(u32::from(b'2')) {
                        i = i.wrapping_add(2);
                    } else if a == Some(u32::from(b'6')) && b == Some(u32::from(b'4')) {
                        flags |= LLONGINT;
                        i = i.wrapping_add(2);
                    } else {
                        if fast {
                            flags &= !spec::FASTINT;
                            i = i.saturating_sub(1);
                        }
                        break;
                    }
                    ch = next_char(fmt, &mut i);
                    continue;
                }
                _ => break,
            }
            ch = next_char(fmt, &mut i);
        }
        if ch == 0 {
            break;
        }
        match ch {
            x if x == u32::from(b'%') => {
                let b = *b"%";
                ret = emit_common(
                    sink,
                    &mut *reader.args,
                    ret,
                    flags,
                    width,
                    0,
                    0,
                    0,
                    1,
                    Body::Bytes(&b),
                    opts,
                )?;
            }
            x if x == u32::from(b'c') || x == u32::from(b'C') => {
                let wide = flags & LONGINT != 0 || x == u32::from(b'C');
                let v = reader.word(argpos)?;
                if C::WIDE {
                    let unit = [v as u32];
                    ret = emit_common(
                        sink,
                        &mut *reader.args,
                        ret,
                        flags,
                        width,
                        0,
                        0,
                        0,
                        1,
                        Body::Units(&unit),
                        opts,
                    )?;
                } else if wide {
                    let wc = v as u32;
                    let size = str::wchar_size(wc, opts.mode)? as i64;
                    let mut mb = [0u8; 4];
                    let k = raven_libc_ctype::wcrtomb(opts.mode, wc, &mut mb)
                        .map_err(|_| FmtErr::Encoding)?;
                    ret = emit_common(
                        sink,
                        &mut *reader.args,
                        ret,
                        flags,
                        width,
                        0,
                        0,
                        0,
                        size,
                        Body::Bytes(mb.get(..k).unwrap_or(&[])),
                        opts,
                    )?;
                } else {
                    let b = [v as u8];
                    ret = emit_common(
                        sink,
                        &mut *reader.args,
                        ret,
                        flags,
                        width,
                        0,
                        0,
                        0,
                        1,
                        Body::Bytes(&b),
                        opts,
                    )?;
                }
            }
            x if x == u32::from(b's') || x == u32::from(b'S') => {
                let wide = flags & LONGINT != 0 || x == u32::from(b'S');
                let ptr = reader.word(argpos)? as usize;
                if ptr == 0 {
                    let n = if prec < 0 {
                        str::NULL_STRING.len()
                    } else {
                        core::cmp::min(str::NULL_STRING.len(), prec as usize)
                    };
                    ret = emit_common(
                        sink,
                        &mut *reader.args,
                        ret,
                        flags,
                        width,
                        0,
                        0,
                        0,
                        n as i64,
                        Body::Bytes(str::NULL_STRING.get(..n).unwrap_or(&[])),
                        opts,
                    )?;
                } else if C::WIDE {
                    if wide {
                        let n = str::wstr_size(reader.args, ptr, prec);
                        ret = emit_common(
                            sink,
                            &mut *reader.args,
                            ret,
                            flags,
                            width,
                            0,
                            0,
                            0,
                            n as i64,
                            Body::WStr { ptr, len: n },
                            opts,
                        )?;
                    } else {
                        let n = str::mb_wstr_size(reader.args, ptr, prec, opts.mode)?;
                        ret = emit_common(
                            sink,
                            &mut *reader.args,
                            ret,
                            flags,
                            width,
                            0,
                            0,
                            0,
                            n as i64,
                            Body::MbStr { ptr, len: n },
                            opts,
                        )?;
                    }
                } else if wide {
                    let n = str::wstr_mb_size(reader.args, ptr, prec, opts.mode)?;
                    ret = emit_common(
                        sink,
                        &mut *reader.args,
                        ret,
                        flags,
                        width,
                        0,
                        0,
                        0,
                        n as i64,
                        Body::WStrMb { ptr, len: n },
                        opts,
                    )?;
                } else {
                    let n = str::cstr_size(reader.args, ptr, prec);
                    ret = emit_common(
                        sink,
                        &mut *reader.args,
                        ret,
                        flags,
                        width,
                        0,
                        0,
                        0,
                        n as i64,
                        Body::NarrowStr { ptr, len: n },
                        opts,
                    )?;
                }
            }
            x if x == u32::from(b'm') => {
                let mut msg = [0u8; 64];
                let n = reader.args.errno_text(opts.serrno, &mut msg);
                let n = core::cmp::min(n, msg.len());
                let size = if prec >= 0 {
                    core::cmp::min(n, prec as usize)
                } else {
                    n
                };
                ret = emit_common(
                    sink,
                    &mut *reader.args,
                    ret,
                    flags,
                    width,
                    0,
                    0,
                    0,
                    size as i64,
                    Body::Bytes(msg.get(..size).unwrap_or(&[])),
                    opts,
                )?;
            }
            x if x == u32::from(b'n') => match opts.pct_n {
                PctN::Abort => return Err(FmtErr::PctNAbort),
                PctN::Reject => return Err(FmtErr::Invalid),
                PctN::Store => {
                    let ptr = reader.word(argpos)? as usize;
                    let size = store_size(flags);
                    reader.args.store_count(ptr, size, ret);
                }
            },
            x if x == u32::from(b'a')
                || x == u32::from(b'A')
                || x == u32::from(b'e')
                || x == u32::from(b'E')
                || x == u32::from(b'f')
                || x == u32::from(b'F')
                || x == u32::from(b'g')
                || x == u32::from(b'G') =>
            {
                let (kind, bits) = if flags & LONGDBL != 0 {
                    (FloatKind::LongDouble, reader.ld_bits(argpos)?)
                } else {
                    (FloatKind::F64, u128::from(reader.f64_bits(argpos)?))
                };
                let conv = FloatConv {
                    ch: x,
                    flags,
                    width,
                    prec,
                    sign,
                };
                ret = float_layout::emit(sink, &conv, FloatBits { kind, bits }, floats, opts, ret)?;
            }
            x if x == u32::from(b'd')
                || x == u32::from(b'i')
                || x == u32::from(b'D')
                || x == u32::from(b'u')
                || x == u32::from(b'U')
                || x == u32::from(b'o')
                || x == u32::from(b'O')
                || x == u32::from(b'x')
                || x == u32::from(b'X')
                || x == u32::from(b'b')
                || x == u32::from(b'B')
                || x == u32::from(b'p') =>
            {
                let mut lflags = flags;
                let mut value;
                let base;
                let mut xdigs_upper = false;
                match x as u8 {
                    b'd' | b'i' => {
                        value = signed_arg(reader.word(argpos)?, flags);
                        if (value as i64) < 0 {
                            value = (value as i64).wrapping_neg() as u64;
                            sign = b'-';
                        }
                        base = 10;
                    }
                    b'D' => {
                        lflags |= LONGINT;
                        value = signed_arg(reader.word(argpos)?, lflags);
                        if (value as i64) < 0 {
                            value = (value as i64).wrapping_neg() as u64;
                            sign = b'-';
                        }
                        base = 10;
                    }
                    b'u' => {
                        value = unsigned_arg(reader.word(argpos)?, flags);
                        base = 10;
                    }
                    b'U' => {
                        lflags |= LONGINT;
                        value = unsigned_arg(reader.word(argpos)?, lflags);
                        base = 10;
                    }
                    b'o' => {
                        value = unsigned_arg(reader.word(argpos)?, flags);
                        base = 8;
                    }
                    b'O' => {
                        lflags |= LONGINT;
                        value = unsigned_arg(reader.word(argpos)?, lflags);
                        base = 8;
                    }
                    b'x' | b'X' => {
                        value = unsigned_arg(reader.word(argpos)?, flags);
                        base = 16;
                        xdigs_upper = x == u32::from(b'X');
                        flags &= !GROUPING;
                    }
                    b'b' | b'B' => {
                        value = unsigned_arg(reader.word(argpos)?, flags);
                        base = 2;
                    }
                    _ => {
                        value = reader.word(argpos)?;
                        base = 16;
                        flags |= INTMAXT;
                        ox = b'x';
                    }
                }
                if base == 16 && flags & ALT != 0 && value != 0 && x != u32::from(b'p') {
                    ox = if xdigs_upper { b'X' } else { b'x' };
                }
                if base == 2 && flags & ALT != 0 && value != 0 {
                    ox = if x == u32::from(b'B') { b'B' } else { b'b' };
                }
                if x != u32::from(b'p')
                    && !(base == 10 && (x == u32::from(b'u') || x == u32::from(b'U')))
                {
                    // Keep the sign of a signed conversion; unsigned and non-decimal
                    // conversions have no sign.
                    if base != 10 {
                        sign = 0;
                    }
                } else {
                    sign = 0;
                }
                let dprec = if prec > 0 { prec } else { 0 };
                let mut dflags = flags;
                if prec >= 0 {
                    dflags &= !ZEROPAD;
                }
                let mut digits = [0u8; spec::INT_BUF];
                let mut start = digits.len();
                if value != 0 || prec != 0 || (dflags & ALT != 0 && base == 8) {
                    start = if dflags & INTMAX_SIZE != 0 {
                        int::ujtoa(
                            value,
                            &mut digits,
                            base,
                            dflags & ALT != 0 && base == 8,
                            xdigs_upper,
                        )
                    } else {
                        int::ultoa(
                            value,
                            &mut digits,
                            base,
                            dflags & ALT != 0 && base == 8,
                            xdigs_upper,
                        )
                    };
                }
                let dlen = digits.len().saturating_sub(start);
                let mut size = dlen as i64;
                let mut grouping = None;
                if dflags & GROUPING != 0 && dlen != 0 {
                    let (g, extra) = Grouping::new(dlen as i32, &opts.numeric, C::WIDE);
                    size = size.saturating_add(extra as i64);
                    grouping = Some(g);
                }
                ret = emit_common(
                    sink,
                    &mut *reader.args,
                    ret,
                    dflags,
                    width,
                    sign,
                    ox,
                    dprec,
                    size,
                    Body::Digits {
                        digits: digits.get(start..).unwrap_or(&[]),
                        grouping,
                    },
                    opts,
                )?;
            }
            _ => {
                if C::WIDE {
                    let unit = [ch];
                    ret = emit_common(
                        sink,
                        &mut *reader.args,
                        ret,
                        flags,
                        width,
                        0,
                        0,
                        0,
                        1,
                        Body::Units(&unit),
                        opts,
                    )?;
                } else {
                    let b = [ch as u8];
                    ret = emit_common(
                        sink,
                        &mut *reader.args,
                        ret,
                        flags,
                        width,
                        0,
                        0,
                        0,
                        1,
                        Body::Bytes(&b),
                        opts,
                    )?;
                }
            }
        }
        let _ = &mut argpos;
        continue 'outer;
    }
    if ret > INT_MAX {
        return Err(FmtErr::Overflow);
    }
    Ok(ret as i32)
}

fn next_char<C: CharUnit>(fmt: &[C], i: &mut usize) -> u32 {
    let v = fmt.get(*i).copied().unwrap_or_default().to_u32();
    *i = (*i).wrapping_add(1);
    v
}

fn store_size(flags: u32) -> StoreSize {
    if flags & LLONGINT != 0 {
        StoreSize::Ll
    } else if flags & SIZET != 0 {
        StoreSize::Z
    } else if flags & PTRDIFFT != 0 {
        StoreSize::T
    } else if flags & INTMAXT != 0 {
        StoreSize::J
    } else if flags & LONGINT != 0 {
        StoreSize::L
    } else if flags & SHORTINT != 0 {
        StoreSize::H
    } else if flags & CHARINT != 0 {
        StoreSize::Hh
    } else {
        StoreSize::Int
    }
}

fn signed_arg(v: u64, flags: u32) -> u64 {
    let x = if flags & INTMAX_SIZE != 0 || flags & LONGINT != 0 {
        v as i64
    } else if flags & SHORTINT != 0 {
        i64::from(v as i16)
    } else if flags & CHARINT != 0 {
        i64::from(v as i8)
    } else {
        i64::from(v as i32)
    };
    x as u64
}

fn unsigned_arg(v: u64, flags: u32) -> u64 {
    if flags & INTMAX_SIZE != 0 || flags & LONGINT != 0 {
        v
    } else if flags & SHORTINT != 0 {
        u64::from(v as u16)
    } else if flags & CHARINT != 0 {
        u64::from(v as u8)
    } else {
        u64::from(v as u32)
    }
}

/// The donor's common emission tail, returning the new output count.
#[allow(clippy::too_many_arguments)] // mirrors the donor's conversion state list
fn emit_common<C: CharUnit, S: Sink<C>, A: FmtEnv>(
    sink: &mut S,
    args: &mut A,
    ret: i64,
    flags: u32,
    width: i32,
    sign: u8,
    ox: u8,
    dprec: i32,
    size: i64,
    body: Body<'_>,
    opts: &FmtOptions<'_>,
) -> Result<i64, FmtErr> {
    let mut realsz = size;
    if i64::from(dprec) > realsz {
        realsz = i64::from(dprec);
    }
    if sign != 0 {
        realsz = realsz.saturating_add(1);
    }
    if ox != 0 {
        realsz = realsz.saturating_add(2);
    }
    let prsize = if i64::from(width) > realsz {
        i64::from(width)
    } else {
        realsz
    };
    if ret.saturating_add(prsize) > INT_MAX {
        return Err(FmtErr::Overflow);
    }
    if flags & (LADJUST | ZEROPAD) == 0 {
        let n = i64::from(width).saturating_sub(realsz);
        if n > 0 {
            sink.pad(C::from_ascii(b' '), n as usize)
                .map_err(|_| FmtErr::Sink)?;
        }
    }
    if sign != 0 {
        sink.write(&[C::from_ascii(sign)])
            .map_err(|_| FmtErr::Sink)?;
    }
    if ox != 0 {
        sink.write(&[C::from_ascii(b'0'), C::from_ascii(ox)])
            .map_err(|_| FmtErr::Sink)?;
    }
    if flags & (LADJUST | ZEROPAD) == ZEROPAD {
        let n = i64::from(width).saturating_sub(realsz);
        if n > 0 {
            sink.pad(C::from_ascii(b'0'), n as usize)
                .map_err(|_| FmtErr::Sink)?;
        }
    }
    let lead = i64::from(dprec).saturating_sub(size);
    if lead > 0 {
        let _ = args;
        sink.pad(C::from_ascii(b'0'), lead as usize)
            .map_err(|_| FmtErr::Sink)?;
    }
    match body {
        Body::Bytes(bytes) => {
            for &b in bytes {
                sink.write(&[C::from_ascii(b)]).map_err(|_| FmtErr::Sink)?;
            }
        }
        Body::Units(units) => {
            for &u in units {
                sink.write(&[C::from_u32(u)]).map_err(|_| FmtErr::Sink)?;
            }
        }
        Body::Digits { digits, grouping } => {
            if let Some(g) = grouping {
                g.print(sink, digits, digits.len(), opts.mode)?;
            } else {
                for &b in digits {
                    sink.write(&[C::from_ascii(b)]).map_err(|_| FmtErr::Sink)?;
                }
            }
        }
        Body::NarrowStr { ptr, len } => {
            str::emit_cstr::<C, S, A>(sink, args, ptr, len)?;
        }
        Body::WStrMb { ptr, len } => {
            str::emit_wstr_mb::<C, S, A>(sink, args, ptr, len, opts.mode)?;
        }
        Body::WStr { ptr, len } => {
            str::emit_wstr::<C, S, A>(sink, args, ptr, len)?;
        }
        Body::MbStr { ptr, len } => {
            str::emit_mb_wstr::<C, S, A>(sink, args, ptr, len, opts.mode)?;
        }
    }
    if flags & LADJUST != 0 {
        let n = i64::from(width).saturating_sub(realsz);
        if n > 0 {
            sink.pad(C::from_ascii(b' '), n as usize)
                .map_err(|_| FmtErr::Sink)?;
        }
    }
    Ok(ret.saturating_add(prsize))
}

/// Validate the positional arguments without writing output (`va_copy` dry run, A-stdio-4).
///
/// The reader is cloned; a floating conversion is not evaluated, so a [`crate::NoFloat`]
/// instantiation does not reject a valid format.
///
/// # Errors
/// [`FmtErr::Invalid`] for a positional gap, mixed positional/sequential use, a type
/// conflict or more than [`crate::NL_ARGMAX`] arguments.
pub fn vformat_dry<C: CharUnit, A: FmtEnv>(fmt: &[C], args: &mut A) -> Result<(), FmtErr> {
    let analysis = pos::analyse(fmt)?;
    if let Some(table) = analysis.table {
        let mut clone = args.clone_state();
        let _ = pos::fetch(&table, &mut clone)?;
    }
    Ok(())
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
#[path = "mod_ut.rs"]
mod tests;
