// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 1990, 1993
//	The Regents of the University of California.  All rights reserved.
//
// This code is derived from software contributed to Berkeley by
// Chris Torek.
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// RavenBSD: transpiled from freebsd:lib/libc/stdio/printf-pos.c @ 133344405f532bd54ff3629a7d4c4e062f01fdb
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
//! The positional-argument pre-scan and type table (`printf-pos.c`).
//!
//! The donor's growable table and its `malloc` collapse into one fixed
//! `[TypeId; NL_ARGMAX + 1]`; a gap, mixed positional/sequential use or a type conflict is
//! [`FmtErr::Invalid`] and nothing is written (T18, `donor-map/04` 5.1.2).
//! `__find_warguments` is the same code over `u32`, so the parser is generic over
//! [`CharUnit`].

#![allow(clippy::arithmetic_side_effects, clippy::indexing_slicing)]
// The scan mirrors the donor's index arithmetic; every table index is checked against
// NL_ARGMAX before use.

use crate::printf::spec::{
    ALT, Analysis, ArgTag, ArgVal, CHARINT, FASTINT, INTMAXT, LADJUST, LLONGINT, LONGDBL, LONGINT,
    PTRDIFFT, SHORTINT, SIZET, TypeId, TypeTable, ZEROPAD,
};
use crate::traits::{CharUnit, VaReader};
use crate::types::{FmtErr, NL_ARGMAX};

/// One item reported by [`walk`].
pub(crate) enum Item {
    /// A `*` width or precision, with its optional positional index.
    Star {
        /// The `m$` index.
        pos: Option<u32>,
    },
    /// A conversion character and the flags collected before it.
    Conv {
        /// The conversion character.
        ch: u32,
        /// The flag bits.
        flags: u32,
        /// The `m$` index of the whole conversion.
        pos: Option<u32>,
    },
}

fn digit(ch: u32) -> bool {
    (0x30..=0x39).contains(&ch)
}

fn to_digit(ch: u32) -> u32 {
    ch.wrapping_sub(0x30)
}

/// Parse a `m$` index after `*`; `i` points just after the `*`.
fn star_index<C: CharUnit>(fmt: &[C], mut i: usize) -> (Option<u32>, usize) {
    let start = i;
    let mut n = 0u32;
    while let Some(&c) = fmt.get(i) {
        let v = c.to_u32();
        if !digit(v) {
            break;
        }
        n = n.saturating_mul(10).saturating_add(to_digit(v));
        i = i.wrapping_add(1);
    }
    if fmt.get(i).map(|c| c.to_u32()) == Some(b'$' as u32) {
        (Some(n), i.wrapping_add(1))
    } else {
        (None, start)
    }
}

/// Walk a format string and report every `*` and conversion to `f`.
///
/// The walk mirrors the donor's `rflag`/`reswitch` state machine; it does not resolve
/// argument values. A NUL ends the format.
///
/// # Errors
/// [`FmtErr::Invalid`] only when the callback returns it.
pub(crate) fn walk<C: CharUnit>(
    fmt: &[C],
    mut f: impl FnMut(Item) -> Result<(), FmtErr>,
) -> Result<(), FmtErr> {
    let mut i = 0usize;
    while i < fmt.len() {
        let c = fmt.get(i).copied().unwrap_or_default().to_u32();
        i = i.wrapping_add(1);
        if c == 0 {
            break;
        }
        if c != u32::from(b'%') {
            continue;
        }
        let mut flags = 0u32;
        let mut pos = None;
        let mut ch = {
            let v = fmt.get(i).copied().unwrap_or_default().to_u32();
            i = i.wrapping_add(1);
            v
        };
        loop {
            match ch {
                0x20 | 0x23 | 0x2b | 0x2d | 0x27 | 0x30 => {
                    match ch {
                        0x20 => {}
                        0x23 => flags |= ALT,
                        0x2b => {}
                        0x2d => flags |= LADJUST,
                        0x27 => {}
                        _ => flags |= ZEROPAD,
                    }
                    ch = {
                        let v = fmt.get(i).copied().unwrap_or_default().to_u32();
                        i = i.wrapping_add(1);
                        v
                    };
                }
                0x2a => {
                    let (idx, next) = star_index(fmt, i);
                    i = next;
                    f(Item::Star { pos: idx })?;
                    ch = {
                        let v = fmt.get(i).copied().unwrap_or_default().to_u32();
                        i = i.wrapping_add(1);
                        v
                    };
                }
                0x2e => {
                    ch = {
                        let v = fmt.get(i).copied().unwrap_or_default().to_u32();
                        i = i.wrapping_add(1);
                        v
                    };
                    if ch == 0x2a {
                        let (idx, next) = star_index(fmt, i);
                        i = next;
                        f(Item::Star { pos: idx })?;
                        ch = {
                            let v = fmt.get(i).copied().unwrap_or_default().to_u32();
                            i = i.wrapping_add(1);
                            v
                        };
                    } else {
                        while digit(ch) {
                            ch = {
                                let v = fmt.get(i).copied().unwrap_or_default().to_u32();
                                i = i.wrapping_add(1);
                                v
                            };
                        }
                    }
                }
                0x31..=0x39 => {
                    let mut n = 0u32;
                    let mut d = ch;
                    loop {
                        n = n.saturating_mul(10).saturating_add(to_digit(d));
                        d = {
                            let v = fmt.get(i).copied().unwrap_or_default().to_u32();
                            i = i.wrapping_add(1);
                            v
                        };
                        if !digit(d) {
                            break;
                        }
                    }
                    if d == u32::from(b'$') {
                        pos = Some(n);
                        ch = {
                            let v = fmt.get(i).copied().unwrap_or_default().to_u32();
                            i = i.wrapping_add(1);
                            v
                        };
                    } else {
                        ch = d;
                    }
                }
                x if x == u32::from(b'L') => {
                    flags |= LONGDBL;
                    ch = {
                        let v = fmt.get(i).copied().unwrap_or_default().to_u32();
                        i = i.wrapping_add(1);
                        v
                    };
                }
                x if x == u32::from(b'h') => {
                    if flags & SHORTINT != 0 {
                        flags &= !SHORTINT;
                        flags |= CHARINT;
                    } else {
                        flags |= SHORTINT;
                    }
                    ch = {
                        let v = fmt.get(i).copied().unwrap_or_default().to_u32();
                        i = i.wrapping_add(1);
                        v
                    };
                }
                x if x == u32::from(b'j') => {
                    flags |= INTMAXT;
                    ch = {
                        let v = fmt.get(i).copied().unwrap_or_default().to_u32();
                        i = i.wrapping_add(1);
                        v
                    };
                }
                x if x == u32::from(b'l') => {
                    if flags & LONGINT != 0 {
                        flags &= !LONGINT;
                        flags |= LLONGINT;
                    } else {
                        flags |= LONGINT;
                    }
                    ch = {
                        let v = fmt.get(i).copied().unwrap_or_default().to_u32();
                        i = i.wrapping_add(1);
                        v
                    };
                }
                x if x == u32::from(b'q') => {
                    flags |= LLONGINT;
                    ch = {
                        let v = fmt.get(i).copied().unwrap_or_default().to_u32();
                        i = i.wrapping_add(1);
                        v
                    };
                }
                x if x == u32::from(b't') => {
                    flags |= PTRDIFFT;
                    ch = {
                        let v = fmt.get(i).copied().unwrap_or_default().to_u32();
                        i = i.wrapping_add(1);
                        v
                    };
                }
                x if x == u32::from(b'z') => {
                    flags |= SIZET;
                    ch = {
                        let v = fmt.get(i).copied().unwrap_or_default().to_u32();
                        i = i.wrapping_add(1);
                        v
                    };
                }
                x if x == u32::from(b'w') => {
                    flags &= !(CHARINT | SHORTINT | LONGINT | LLONGINT | INTMAXT);
                    let fast = fmt.get(i).map(|c| c.to_u32()) == Some(u32::from(b'f'));
                    if fast {
                        flags |= FASTINT;
                        i = i.wrapping_add(1);
                    } else {
                        flags &= !FASTINT;
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
                            flags &= !FASTINT;
                            i = i.saturating_sub(1);
                        }
                        break;
                    }
                    ch = {
                        let v = fmt.get(i).copied().unwrap_or_default().to_u32();
                        i = i.wrapping_add(1);
                        v
                    };
                }
                _ => break,
            }
        }
        if ch == 0 {
            break;
        }
        f(Item::Conv { ch, flags, pos })?;
    }
    Ok(())
}

/// Whether a conversion consumes an argument.
fn consumes(ch: u32) -> bool {
    matches!(
        ch as u8,
        b'B' | b'b'
            | b'C'
            | b'c'
            | b'D'
            | b'd'
            | b'i'
            | b'a'
            | b'A'
            | b'e'
            | b'E'
            | b'f'
            | b'F'
            | b'g'
            | b'G'
            | b'n'
            | b'O'
            | b'o'
            | b'p'
            | b'S'
            | b's'
            | b'U'
            | b'u'
            | b'X'
            | b'x'
    )
}

/// The type-table id of one conversion, or `None` when it consumes no argument.
fn type_of(ch: u32, flags: u32) -> Option<TypeId> {
    let signed = if flags & INTMAXT != 0 {
        TypeId::IntMaxT
    } else if flags & SIZET != 0 {
        TypeId::SSizeT
    } else if flags & PTRDIFFT != 0 {
        TypeId::PtrDiffT
    } else if flags & LLONGINT != 0 {
        TypeId::LLong
    } else if flags & LONGINT != 0 {
        TypeId::Long
    } else {
        TypeId::Int
    };
    let unsigned = if flags & INTMAXT != 0 {
        TypeId::UIntMaxT
    } else if flags & SIZET != 0 || flags & PTRDIFFT != 0 {
        TypeId::SizeT
    } else if flags & LLONGINT != 0 {
        TypeId::ULLong
    } else if flags & LONGINT != 0 {
        TypeId::ULong
    } else {
        TypeId::UInt
    };
    match ch as u8 {
        b'D' => Some(TypeId::Long),
        b'd' | b'i' => Some(signed),
        b'O' => Some(TypeId::ULong),
        b'o' | b'u' => Some(unsigned),
        b'U' => Some(TypeId::ULong),
        b'X' | b'x' => Some(unsigned),
        b'B' | b'b' => Some(unsigned),
        b'C' | b'c' => {
            if flags & LONGINT != 0 {
                Some(TypeId::WInt)
            } else {
                Some(TypeId::Int)
            }
        }
        b'S' | b's' => {
            if flags & LONGINT != 0 {
                Some(TypeId::PWChar)
            } else {
                Some(TypeId::PChar)
            }
        }
        b'p' => Some(TypeId::PVoid),
        b'a' | b'A' | b'e' | b'E' | b'f' | b'F' | b'g' | b'G' => {
            if flags & LONGDBL != 0 {
                Some(TypeId::LongDouble)
            } else {
                Some(TypeId::Double)
            }
        }
        b'n' => {
            if flags & INTMAXT != 0 {
                Some(TypeId::PIntMaxT)
            } else if flags & PTRDIFFT != 0 {
                Some(TypeId::PPtrDiffT)
            } else if flags & SIZET != 0 {
                Some(TypeId::PSSizeT)
            } else if flags & LLONGINT != 0 {
                Some(TypeId::PLLong)
            } else if flags & LONGINT != 0 {
                Some(TypeId::PLong)
            } else if flags & SHORTINT != 0 {
                Some(TypeId::PShort)
            } else if flags & CHARINT != 0 {
                Some(TypeId::PSChar)
            } else {
                Some(TypeId::PInt)
            }
        }
        _ => None,
    }
}

/// Pre-scan `fmt`; returns the filled type table when positional arguments are used.
///
/// # Errors
/// [`FmtErr::Invalid`] for a gap, mixed positional/sequential use, a type conflict or more
/// than [`NL_ARGMAX`] arguments.
pub(crate) fn analyse<C: CharUnit>(fmt: &[C]) -> Result<Analysis, FmtErr> {
    let mut table = TypeTable::new();
    let mut saw_pos = false;
    let mut saw_plain = false;
    let mut error = None;
    walk(fmt, |item| {
        match item {
            Item::Star { pos } => {
                if let Some(n) = pos {
                    if saw_plain {
                        error = Some(FmtErr::Invalid);
                        return Err(FmtErr::Invalid);
                    }
                    saw_pos = true;
                    table.nextarg = n;
                    table.add(TypeId::Int)?;
                } else if saw_pos {
                    error = Some(FmtErr::Invalid);
                    return Err(FmtErr::Invalid);
                } else {
                    saw_plain = true;
                }
            }
            Item::Conv { ch, flags, pos } => {
                if let Some(n) = pos {
                    if saw_plain {
                        error = Some(FmtErr::Invalid);
                        return Err(FmtErr::Invalid);
                    }
                    saw_pos = true;
                    table.nextarg = n;
                    if let Some(id) = type_of(ch, flags) {
                        table.add(id)?;
                    }
                } else if consumes(ch) {
                    if saw_pos {
                        error = Some(FmtErr::Invalid);
                        return Err(FmtErr::Invalid);
                    }
                    saw_plain = true;
                }
            }
        }
        Ok(())
    })?;
    if let Some(e) = error {
        return Err(e);
    }
    if !saw_pos {
        return Ok(Analysis { table: None });
    }
    let mut n = 1u32;
    while n <= table.tablemax {
        let id = table
            .types
            .get(n as usize)
            .copied()
            .unwrap_or(TypeId::Unused);
        if id == TypeId::Unused {
            return Err(FmtErr::Invalid);
        }
        n = n.saturating_add(1);
    }
    Ok(Analysis { table: Some(table) })
}

/// Fetch every argument of the table once, in order, from the cloned reader.
///
/// # Errors
/// [`FmtErr::Invalid`] for a gap or a table index outside [`NL_ARGMAX`].
pub(crate) fn fetch<A: VaReader>(
    table: &TypeTable,
    reader: &mut A,
) -> Result<([ArgVal; NL_ARGMAX + 1], [ArgTag; NL_ARGMAX + 1]), FmtErr> {
    let mut values = [ArgVal::word(0); NL_ARGMAX + 1];
    let mut tags = [ArgTag::Int; NL_ARGMAX + 1];
    let mut n = 1u32;
    while n <= table.tablemax {
        let id = table
            .types
            .get(n as usize)
            .copied()
            .unwrap_or(TypeId::Unused);
        let (value, tag) = match id.arg() {
            crate::printf::spec::ArgClass::Unused => return Err(FmtErr::Invalid),
            crate::printf::spec::ArgClass::Integer => {
                (ArgVal::word(reader.next_u64()), ArgTag::Int)
            }
            crate::printf::spec::ArgClass::Double => {
                (ArgVal::word(reader.next_f64_bits()), ArgTag::F64)
            }
            crate::printf::spec::ArgClass::LongDouble => {
                (ArgVal::wide(reader.next_ld_bits()), ArgTag::Ld)
            }
            crate::printf::spec::ArgClass::Pointer => {
                (ArgVal::word(reader.next_u64()), ArgTag::Ptr)
            }
            crate::printf::spec::ArgClass::WInt => {
                (ArgVal::word(reader.next_u64() as u32 as u64), ArgTag::WInt)
            }
        };
        if let Some(slot) = values.get_mut(n as usize) {
            *slot = value;
        }
        if let Some(slot) = tags.get_mut(n as usize) {
            *slot = tag;
        }
        n = n.saturating_add(1);
    }
    Ok((values, tags))
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
#[path = "pos_ut.rs"]
mod tests;
