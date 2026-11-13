// SPDX-License-Identifier: BSD-2-Clause
//
// Copyright 1996, 1997, 1998, 1999, 2000 John D. Polstra.
// Copyright 2003 Alexander Kabaev <kan@FreeBSD.ORG>.
// Copyright 2009-2013 Konstantin Belousov <kib@FreeBSD.ORG>.
// Copyright 2012 John Marino <draco@marino.st>.
// Copyright 2014-2017 The FreeBSD Foundation
// All rights reserved.
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// RavenBSD: transpiled from freebsd:libexec/rtld-elf/rtld.c @ 133344405f532bd54ff3629a77d4c4e062f01fdb
// (file revision tag: 133344405f532bd54ff3629a77d4c4e062f01fdb)
//
// Redistribution and use in source and binary forms, with or without
// modification, are permitted provided that the following conditions
// are met:
// 1. Redistributions of source code must retain the above copyright
//    notice, this list of conditions and the following disclaimer.
// 2. Redistributions in binary form must reproduce the above copyright
//    notice, this list of conditions and the following disclaimer in the
//    documentation and/or other materials provided with the distribution.
//
// THIS SOFTWARE IS PROVIDED BY THE AUTHOR AND CONTRIBUTORS ``AS IS'' AND
// ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT LIMITED TO, THE
// IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE
// ARE DISCLAIMED.  IN NO EVENT SHALL THE AUTHOR OR CONTRIBUTORS BE LIABLE
// FOR ANY DIRECT, INDIRECT, INCIDENTAL, SPECIAL, EXEMPLARY, OR CONSEQUENTIAL
// DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS
// OR SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION)
// HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY, WHETHER IN CONTRACT, STRICT
// LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY
// OUT OF THE USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF
// SUCH DAMAGE.
//
//! The `ldd` line formatter.
//!
//! `format_trace` returns bytes; it never prints. The default first format is
//! `\t%o => %p (%x)\n` and the second `\t%o (%x)\n` (13 3.11). `%A` is the program name, `%a`
//! the main program's path, `%o` the requested name, `%p` the resolved path and `%x` the load bias
//! in lower hexadecimal with a `0x` prefix. `\n` and `\t` are escapes; an unknown `%z` is kept
//! literally.

use alloc::vec::Vec;

/// One line's inputs.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TraceEntry {
    /// The `DT_NEEDED` name.
    pub name: Vec<u8>,
    /// The resolved path, or `not found`.
    pub path: Vec<u8>,
    /// The load bias.
    pub bias: u64,
    /// `LD_TRACE_LOADED_OBJECTS_PROGNAME`.
    pub progname: Vec<u8>,
    /// The main program's path.
    pub argv0: Vec<u8>,
}

/// The default first format.
pub const DEFAULT_FMT1: &[u8] = b"\t%o => %p (%x)\n";
/// The default second format.
pub const DEFAULT_FMT2: &[u8] = b"\t%o (%x)\n";

fn hex(value: u64, out: &mut Vec<u8>) {
    out.extend_from_slice(b"0x");
    if value == 0 {
        out.push(b'0');
        return;
    }
    let mut started = false;
    let mut shift = 60u32;
    loop {
        let digit = ((value >> shift) & 0xf) as u8;
        if digit != 0 || started {
            started = true;
            out.push(if digit < 10 {
                b'0'.wrapping_add(digit)
            } else {
                b'a'.wrapping_add(digit).wrapping_sub(10)
            });
        }
        if shift == 0 {
            break;
        }
        shift = shift.wrapping_sub(4);
    }
}

fn push(out: &mut Vec<u8>, bytes: &[u8]) {
    out.extend_from_slice(bytes);
}

fn format_one(fmt: &[u8], entry: &TraceEntry) -> Vec<u8> {
    let mut out = Vec::new();
    let mut index = 0usize;
    while index < fmt.len() {
        let Some(byte) = fmt.get(index).copied() else {
            break;
        };
        match byte {
            b'\\' => {
                index = index.saturating_add(1);
                match fmt.get(index).copied() {
                    Some(b'n') => out.push(b'\n'),
                    Some(b't') => out.push(b'\t'),
                    Some(other) => out.push(other),
                    None => {}
                }
            }
            b'%' => {
                index = index.saturating_add(1);
                match fmt.get(index).copied() {
                    Some(b'A') => push(&mut out, &entry.progname),
                    Some(b'a') => push(&mut out, &entry.argv0),
                    Some(b'o') => push(&mut out, &entry.name),
                    Some(b'p') => push(&mut out, &entry.path),
                    Some(b'x') => hex(entry.bias, &mut out),
                    Some(b'%') => out.push(b'%'),
                    Some(other) => {
                        out.push(b'%');
                        out.push(other);
                    }
                    None => out.push(b'%'),
                }
            }
            other => out.push(other),
        }
        index = index.saturating_add(1);
    }
    out
}

/// Format one traced object.
///
/// The first format is used for a name starting with `lib`, the second otherwise; a missing
/// format falls back to its default.
pub fn format_trace(fmt1: Option<&[u8]>, fmt2: Option<&[u8]>, entry: &TraceEntry) -> Vec<u8> {
    let library = entry.name.starts_with(b"lib");
    let chosen = if library {
        fmt1.unwrap_or(DEFAULT_FMT1)
    } else {
        fmt2.unwrap_or(DEFAULT_FMT2)
    };
    format_one(chosen, entry)
}

#[cfg(test)]
#[path = "trace_ut.rs"]
mod tests;
