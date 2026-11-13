// SPDX-License-Identifier: LicenseRef-gdtoa
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
// Copyright (C) 2000 by Lucent Technologies
// All Rights Reserved
//
// RavenBSD: transpiled from netbsd:lib/libc/gdtoa/hd_init.c @ 36eddebcc36cb0775aa2962005b9c05419a6ff01
// (file revision tag: $NetBSD: hd_init.c,v 1.2 2006/01/25 15:27:42 kleink Exp $)
//
// The author of this software is David M. Gay.
//
// Permission to use, copy, modify, and distribute this software and
// its documentation for any purpose and without fee is hereby
// granted, provided that the above copyright notice appear in all
// copies and that both that the copyright notice and this
// permission notice and warranty disclaimer appear in supporting
// documentation, and that the name of Lucent or any of its entities
// not be used in advertising or publicity pertaining to
// distribution of the software without specific, written prior
// permission.
//
// LUCENT DISCLAIMS ALL WARRANTIES WITH REGARD TO THIS SOFTWARE,
// INCLUDING ALL IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS.
// IN NO EVENT SHALL LUCENT OR ANY OF ITS ENTITIES BE LIABLE FOR ANY
// SPECIAL, INDIRECT OR CONSEQUENTIAL DAMAGES OR ANY DAMAGES
// WHATSOEVER RESULTING FROM LOSS OF USE, DATA OR PROFITS, WHETHER
// IN AN ACTION OF CONTRACT, NEGLIGENCE OR OTHER TORTIOUS ACTION,
// ARISING OUT OF OR IN CONNECTION WITH THE USE OR PERFORMANCE OF
// THIS SOFTWARE.
//
// Please send bug reports to David M. Gay (dmg at acm dot org,
// with " at " changed at "@" and " dot " changed to ".").
//! `hd_init.c` of NetBSD gdtoa: the hexadecimal digit table, as a `const`.
//!
//! Transpiled from `netbsd:lib/libc/gdtoa/hd_init.c` (David M. Gay, Lucent Technologies); see
//! `PROVENANCE.toml`. The donor's one-time `hexdig_init_D2A()` (a benign C data race) becomes a
//! compile-time table; the values are the donor's: `'0'..'9'` 0x10..0x19, `'a'..'f'` and
//! `'A'..'F'` 0x1a..0x1f, every other byte 0.

/// The 256-entry `hexdig[]` table.
pub(crate) const HEXDIG: [u8; 256] = build();

/// Fill the table exactly as the donor's `htinit` calls do.
#[allow(clippy::arithmetic_side_effects, clippy::indexing_slicing)] // const table construction, no runtime path
const fn build() -> [u8; 256] {
    let mut table = [0u8; 256];
    let digits = *b"0123456789";
    let lower = *b"abcdef";
    let upper = *b"ABCDEF";
    let mut i = 0usize;
    while i < 10 {
        table[digits[i] as usize] = (0x10 + i) as u8;
        i += 1;
    }
    i = 0;
    while i < 6 {
        table[lower[i] as usize] = (0x1a + i) as u8;
        table[upper[i] as usize] = (0x1a + i) as u8;
        i += 1;
    }
    table
}

#[cfg(test)]
#[path = "hexdig_ut.rs"]
mod tests;
