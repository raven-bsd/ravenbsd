// SPDX-License-Identifier: BSD-2-Clause
//
// Copyright 1996-1998 John D. Polstra.
// All rights reserved.
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// RavenBSD: transpiled from freebsd:libexec/rtld-elf/debug.c @ 133344405f532bd54ff3629a77d4c4e062f01fdb
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
// THIS SOFTWARE IS PROVIDED BY THE AUTHOR ``AS IS'' AND ANY EXPRESS OR
// IMPLIED WARRANTIES, INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES
// OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE DISCLAIMED.
// IN NO EVENT SHALL THE AUTHOR BE LIABLE FOR ANY DIRECT, INDIRECT,
// INCIDENTAL, SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT
// NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR SERVICES; LOSS OF USE,
// DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY
// THEORY OF LIABILITY, WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT
// (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE USE OF
// THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.
//
//! `LD_DEBUG` word parsing.
//!
//! The donor has a boolean `debug` flag; the word list is PROPOSED by the area document (13 3.1)
//! and lives here. This module only parses; the loader prints.

/// A set of `LD_DEBUG` words.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DebugWords(pub u32);

impl DebugWords {
    /// Object loading.
    pub const FILES: u32 = 1 << 0;
    /// Relocation processing.
    pub const RELOC: u32 = 1 << 1;
    /// Symbol binding.
    pub const BIND: u32 = 1 << 2;
    /// Symbol table dumps.
    pub const SYMBOLS: u32 = 1 << 3;
    /// Version checks.
    pub const VERSIONS: u32 = 1 << 4;
    /// TLS allocation.
    pub const TLS: u32 = 1 << 5;
    /// Init and fini calls.
    pub const INIT: u32 = 1 << 6;
    /// Library search.
    pub const LIBS: u32 = 1 << 7;
    /// Statistics.
    pub const STATISTICS: u32 = 1 << 8;
    /// The tracing ring.
    pub const TRACE: u32 = 1 << 9;
    /// Everything.
    pub const ALL: u32 = Self::FILES
        | Self::RELOC
        | Self::BIND
        | Self::SYMBOLS
        | Self::VERSIONS
        | Self::TLS
        | Self::INIT
        | Self::LIBS
        | Self::STATISTICS
        | Self::TRACE;

    /// Parse a comma-separated list; unknown words are ignored, `1` means `all`, and an empty
    /// value selects nothing.
    pub fn parse(text: &[u8]) -> Self {
        if text == b"1" {
            return Self(Self::ALL);
        }
        let mut words = 0u32;
        for word in text.split(|byte| *byte == b',') {
            let word = trim(word);
            let bits = match word {
                b"files" => Self::FILES,
                b"reloc" => Self::RELOC,
                b"bind" => Self::BIND,
                b"symbols" => Self::SYMBOLS,
                b"versions" => Self::VERSIONS,
                b"tls" => Self::TLS,
                b"init" => Self::INIT,
                b"libs" => Self::LIBS,
                b"statistics" => Self::STATISTICS,
                b"trace" => Self::TRACE,
                b"all" => Self::ALL,
                _ => 0,
            };
            words |= bits;
        }
        Self(words)
    }

    /// Whether every bit of `other` is set.
    pub fn contains(self, other: u32) -> bool {
        self.0 & other == other
    }

    /// Whether any word is set.
    pub fn is_empty(self) -> bool {
        self.0 == 0
    }
}

fn trim(mut bytes: &[u8]) -> &[u8] {
    while let Some((first, rest)) = bytes.split_first() {
        if first.is_ascii_whitespace() {
            bytes = rest;
        } else {
            break;
        }
    }
    while let Some((last, rest)) = bytes.split_last() {
        if last.is_ascii_whitespace() {
            bytes = rest;
        } else {
            break;
        }
    }
    bytes
}

#[cfg(test)]
#[path = "debug_ut.rs"]
mod tests;
