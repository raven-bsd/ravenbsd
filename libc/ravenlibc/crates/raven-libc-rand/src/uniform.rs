// SPDX-License-Identifier: ISC
// Copyright (c) 2008, Damien Miller <djm@openbsd.org>
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// RavenBSD: transpiled from openbsd:lib/libc/crypt/arc4random_uniform.c @ 4ce71f2817f675745b0596aaf04de4a122b1cc52
// (file revision tag: $OpenBSD: arc4random_uniform.c,v 1.3 2019/01/20 02:59:07 bcook Exp $)
//
// Permission to use, copy, modify, and distribute this software for any
// purpose with or without fee is hereby granted, provided that the above
// copyright notice and this permission notice appear in all copies.
//
// THE SOFTWARE IS PROVIDED "AS IS" AND THE AUTHOR DISCLAIMS ALL WARRANTIES
// WITH REGARD TO THIS SOFTWARE INCLUDING ALL IMPLIED WARRANTIES OF
// MERCHANTABILITY AND FITNESS. IN NO EVENT SHALL THE AUTHOR BE LIABLE FOR
// ANY SPECIAL, DIRECT, INDIRECT, OR CONSEQUENTIAL DAMAGES OR ANY DAMAGES
// WHATSOEVER RESULTING FROM LOSS OF USE, DATA OR PROFITS, WHETHER IN AN
// ACTION OF CONTRACT, NEGLIGENCE OR OTHER TORTIOUS ACTION, ARISING OUT OF
// OR IN CONNECTION WITH THE USE OR PERFORMANCE OF THIS SOFTWARE.

//! Uniform values in `[0, upper_bound)`, without modulo bias.

/// Draw a uniform value below `upper_bound` from `next`.
///
/// Values below `2^32 mod upper_bound` are discarded; `upper_bound < 2` returns 0 without a draw.
pub fn uniform_with(upper_bound: u32, next: &mut dyn FnMut() -> u32) -> u32 {
    if upper_bound < 2 {
        return 0;
    }
    let minimum = upper_bound.wrapping_neg() % upper_bound;
    loop {
        let value = next();
        if value >= minimum {
            return value % upper_bound;
        }
    }
}

#[cfg(test)]
#[path = "uniform_ut.rs"]
mod tests;
