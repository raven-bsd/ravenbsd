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
//! The two symbol-table hash functions.

use core::num::Wrapping;

/// The System V `DT_HASH` hash of a name.
///
/// The name is read as bytes, never as a signed `char`: `0x80..=0xff` must not sign-extend. The
/// shift and add wrap in `u32` and the result has its top four bits cleared.
pub fn elf_hash(name: &[u8]) -> u32 {
    let mut hash = Wrapping(0u32);
    for byte in name {
        hash = (hash << 4) + Wrapping(u32::from(*byte));
        hash ^= (hash >> 24) & Wrapping(0xf0);
    }
    (hash & Wrapping(0x0fff_ffff)).0
}

/// The GNU (`DT_GNU_HASH`) hash: Daniel J. Bernstein's hash clipped to 32 bits.
pub fn gnu_hash(name: &[u8]) -> u32 {
    let mut hash = Wrapping(5381u32);
    for byte in name {
        hash = hash * Wrapping(33) + Wrapping(u32::from(*byte));
    }
    hash.0
}

#[cfg(test)]
#[path = "hash_ut.rs"]
mod tests;
