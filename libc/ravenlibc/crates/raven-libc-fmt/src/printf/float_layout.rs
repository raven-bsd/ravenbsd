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
// (the fp_begin/fp_common labels and the %e %f %g / %a cases, lines 698-812 and 1020-1060)
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
//! The floating-point layout: digits, decimal point, exponent, width and sign.
//!
//! The digit strings come from [`FloatFmt`] (mode 2 for `%e`/`%g`, mode 3 for `%f`) and
//! [`FloatFmt::hex_digits`] for `%a`; this module owns the FreeBSD `%g` switch, the
//! `#`/trailing-zero rules, grouping of the integer part, `inf`/`nan` handling and the
//! `*float*` dummy text. The two digit buffers live only in this `#[inline(never)]` path,
//! so an integer-only instantiation keeps a small frame.

#![allow(clippy::arithmetic_side_effects, clippy::indexing_slicing)]
// The donor's field arithmetic is mirrored; every output-size addition has its INT_MAX
// check, and the buffers are sized by DEC_BUF_F64/DEC_BUF_LD.

use crate::printf::group::{Grouping, write_ascii, write_mb};
use crate::printf::int::exponent;
use crate::printf::spec::{ALT, DEFPREC, FPT, GROUPING, LADJUST, ZEROPAD};
use crate::traits::{CharUnit, FloatFmt, Sink};
use crate::types::{
    DEC_BUF_F64, DEC_BUF_LD, DigitsOut, FloatBits, FloatClass, FloatKind, FmtErr, FmtOptions,
};

/// One parsed floating-point conversion.
pub(crate) struct FloatConv {
    /// The conversion character (`a A e E f F g G`).
    pub(crate) ch: u32,
    /// The collected flag bits.
    pub(crate) flags: u32,
    /// The field width.
    pub(crate) width: i32,
    /// The precision; negative means absent.
    pub(crate) prec: i32,
    /// The sign character already selected by `+`/space.
    pub(crate) sign: u8,
}

const INT_MAX: i64 = i32::MAX as i64;

/// Render one float conversion and return the updated output count.
///
/// A [`FmtErr::FloatUnavailable`] hook prints the literal `*float*` with the field width
/// and continues (`donor-map/16` R-13).
///
/// # Errors
/// [`FmtErr::Invalid`] for [`crate::NoFloat`], [`FmtErr::NoMem`] when the hook's buffer is
/// exhausted, [`FmtErr::Sink`] when the sink fails, and [`FmtErr::Overflow`] when the result
/// would exceed `INT_MAX`.
#[inline(never)]
pub(crate) fn emit<C: CharUnit, S: Sink<C>, F: FloatFmt>(
    sink: &mut S,
    conv: &FloatConv,
    bits: FloatBits,
    floats: &F,
    opts: &FmtOptions<'_>,
    ret: i64,
) -> Result<i64, FmtErr> {
    match bits.kind {
        FloatKind::LongDouble => {
            let mut buf = [0u8; DEC_BUF_LD];
            emit_inner(sink, conv, bits, floats, opts, ret, &mut buf)
        }
        FloatKind::F32 | FloatKind::F64 => {
            let mut buf = [0u8; DEC_BUF_F64];
            emit_inner(sink, conv, bits, floats, opts, ret, &mut buf)
        }
    }
}

fn emit_inner<C: CharUnit, S: Sink<C>, F: FloatFmt>(
    sink: &mut S,
    conv: &FloatConv,
    bits: FloatBits,
    floats: &F,
    opts: &FmtOptions<'_>,
    ret: i64,
    buf: &mut [u8],
) -> Result<i64, FmtErr> {
    let ch = conv.ch as u8;
    let mut flags = conv.flags;
    let mut prec = conv.prec;
    let width = conv.width;
    let mut sign = conv.sign;
    let mut ox = 0u8;
    let mut expchar: u8;
    let out: DigitsOut;
    let mut ndigits = 0usize;

    if ch == b'a' || ch == b'A' {
        ox = if ch == b'a' { b'x' } else { b'X' };
        expchar = if ch == b'a' { b'p' } else { b'P' };
        if prec >= 0 {
            prec = prec.saturating_add(1);
        }
        let nd = if prec < 0 { -1 } else { prec };
        let upper = ch == b'A';
        match floats.hex_digits(bits, upper, nd, buf) {
            Ok(d) => {
                out = d;
                if prec < 0 {
                    prec = d.len as i32;
                }
            }
            Err(FmtErr::FloatUnavailable) => return stub(sink, conv, ret),
            Err(e) => return Err(e),
        }
        if out.class == FloatClass::Inf || out.class == FloatClass::Nan {
            ox = 0;
        }
    } else {
        if ch == b'g' || ch == b'G' {
            expchar = if ch == b'g' { b'e' } else { b'E' };
            if prec == 0 {
                prec = 1;
            }
        } else if ch == b'e' || ch == b'E' {
            expchar = ch;
            if prec < 0 {
                prec = DEFPREC.saturating_add(1);
            } else {
                prec = prec.saturating_add(1);
            }
        } else {
            expchar = 0;
        }
        if prec < 0 {
            prec = DEFPREC;
        }
        let mode = if expchar != 0 { 2u8 } else { 3u8 };
        match floats.dec_digits(bits, mode, prec, buf) {
            Ok(d) => out = d,
            Err(FmtErr::FloatUnavailable) => return stub(sink, conv, ret),
            Err(e) => return Err(e),
        }
    }

    if out.neg {
        sign = b'-';
    }
    let mut size: i64;
    let mut gs: Option<Grouping<'_>> = None;
    let mut expstr = [0u8; 8];
    let mut expsize = 0usize;
    if out.class == FloatClass::Inf || out.class == FloatClass::Nan {
        if out.class == FloatClass::Nan {
            sign = 0;
        }
        size = 3;
        flags &= !ZEROPAD;
    } else {
        flags |= FPT;
        ndigits = out.len;
        let digits = buf.get(..ndigits).unwrap_or(&[]);
        if ch == b'g' || ch == b'G' {
            if out.decpt > -4 && out.decpt <= prec {
                expchar = 0;
                if flags & ALT != 0 {
                    prec = prec.saturating_sub(out.decpt);
                } else {
                    prec = ndigits as i32 - out.decpt;
                }
                if prec < 0 {
                    prec = 0;
                }
            } else if flags & ALT == 0 {
                prec = ndigits as i32;
            }
        }
        if expchar != 0 {
            expsize = exponent(&mut expstr, out.decpt.wrapping_sub(1), expchar);
            size = expsize as i64 + i64::from(prec);
            if prec > 1 || flags & ALT != 0 {
                size = size.saturating_add(opts.numeric.decimal_point.len() as i64);
            }
        } else {
            if out.decpt > 0 {
                size = i64::from(out.decpt);
            } else {
                size = 1;
            }
            if prec != 0 || flags & ALT != 0 {
                size = size
                    .saturating_add(i64::from(prec))
                    .saturating_add(opts.numeric.decimal_point.len() as i64);
            }
            if flags & GROUPING != 0 && out.decpt > 0 {
                let (g, extra) = Grouping::new(out.decpt, &opts.numeric, C::WIDE);
                size = size.saturating_add(extra as i64);
                gs = Some(g);
            }
        }
        let _ = digits;
    }

    let mut realsz = size;
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

    let pad_blank = if flags & (LADJUST | ZEROPAD) == 0 {
        i64::from(width).saturating_sub(realsz)
    } else {
        0
    };
    if pad_blank > 0 {
        sink.pad(C::from_ascii(b' '), pad_blank as usize)
            .map_err(|_| FmtErr::Sink)?;
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

    if out.class == FloatClass::Inf || out.class == FloatClass::Nan {
        let text: &[u8; 3] = if out.class == FloatClass::Nan {
            if ch.is_ascii_uppercase() {
                b"NAN"
            } else {
                b"nan"
            }
        } else if ch.is_ascii_uppercase() {
            b"INF"
        } else {
            b"inf"
        };
        write_ascii(sink, text)?;
    } else if expchar == 0 {
        let digits = buf.get(..ndigits).unwrap_or(&[]);
        let mut cp = 0usize;
        if out.decpt <= 0 {
            sink.write(&[C::from_ascii(b'0')])
                .map_err(|_| FmtErr::Sink)?;
            if prec != 0 || flags & ALT != 0 {
                write_mb(sink, opts.numeric.decimal_point, opts.mode)?;
            }
            let zeros = i64::from(out.decpt).saturating_neg();
            if zeros > 0 {
                sink.pad(C::from_ascii(b'0'), zeros as usize)
                    .map_err(|_| FmtErr::Sink)?;
            }
            let frac = i64::from(prec).saturating_add(i64::from(out.decpt));
            if frac > 0 {
                and_pad(sink, digits, cp, frac as usize)?;
            }
        } else {
            if let Some(g) = gs.take() {
                cp = g.print(sink, digits, ndigits, opts.mode)?;
            } else {
                and_pad(sink, digits, cp, out.decpt as usize)?;
                cp = core::cmp::min(digits.len(), out.decpt as usize);
            }
            if prec != 0 || flags & ALT != 0 {
                write_mb(sink, opts.numeric.decimal_point, opts.mode)?;
            }
            if prec > 0 {
                and_pad(sink, digits, cp, prec as usize)?;
            }
        }
    } else {
        let digits = buf.get(..ndigits).unwrap_or(&[]);
        if prec > 1 || flags & ALT != 0 {
            write_ascii(sink, digits.get(..1).unwrap_or(&[]))?;
            write_mb(sink, opts.numeric.decimal_point, opts.mode)?;
            if ndigits > 1 {
                write_ascii(sink, digits.get(1..ndigits).unwrap_or(&[]))?;
            }
            let pad = i64::from(prec).saturating_sub(ndigits as i64);
            if pad > 0 {
                sink.pad(C::from_ascii(b'0'), pad as usize)
                    .map_err(|_| FmtErr::Sink)?;
            }
        } else {
            write_ascii(sink, digits.get(..1).unwrap_or(&[]))?;
        }
        write_ascii(sink, expstr.get(..expsize).unwrap_or(&[]))?;
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

/// The R-13 dummy: print `*float*` (width and the generic field rules apply) and continue.
fn stub<C: CharUnit, S: Sink<C>>(sink: &mut S, conv: &FloatConv, ret: i64) -> Result<i64, FmtErr> {
    let text = b"*float*";
    let size = text.len() as i64;
    let realsz = size;
    let prsize = if i64::from(conv.width) > realsz {
        i64::from(conv.width)
    } else {
        realsz
    };
    if ret.saturating_add(prsize) > INT_MAX {
        return Err(FmtErr::Overflow);
    }
    if conv.flags & (LADJUST | ZEROPAD) == 0 {
        let n = i64::from(conv.width).saturating_sub(realsz);
        if n > 0 {
            sink.pad(C::from_ascii(b' '), n as usize)
                .map_err(|_| FmtErr::Sink)?;
        }
    }
    if conv.flags & (LADJUST | ZEROPAD) == ZEROPAD {
        let n = i64::from(conv.width).saturating_sub(realsz);
        if n > 0 {
            sink.pad(C::from_ascii(b'0'), n as usize)
                .map_err(|_| FmtErr::Sink)?;
        }
    }
    write_ascii(sink, text)?;
    if conv.flags & LADJUST != 0 {
        let n = i64::from(conv.width).saturating_sub(realsz);
        if n > 0 {
            sink.pad(C::from_ascii(b' '), n as usize)
                .map_err(|_| FmtErr::Sink)?;
        }
    }
    Ok(ret.saturating_add(prsize))
}

/// The donor's `PRINTANDPAD(p, ep, len, zeroes)` at a digit offset.
fn and_pad<C: CharUnit, S: Sink<C>>(
    sink: &mut S,
    digits: &[u8],
    off: usize,
    len: usize,
) -> Result<(), FmtErr> {
    let avail = digits.len().saturating_sub(off);
    let take = if avail < len { avail } else { len };
    if take > 0 {
        write_ascii(
            sink,
            digits.get(off..off.saturating_add(take)).unwrap_or(&[]),
        )?;
    }
    let pad = len.saturating_sub(take);
    if pad > 0 {
        sink.pad(C::from_ascii(b'0'), pad)
            .map_err(|_| FmtErr::Sink)?;
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
#[path = "float_layout_ut.rs"]
mod tests;
