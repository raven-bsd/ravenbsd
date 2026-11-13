// SPDX-License-Identifier: MIT
// Copyright © 2005-2020 Rich Felker, et al.
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// RavenBSD: transpiled from musl:src/string/explicit_bzero.c @ 7813e9119eb5fe681b6750b8a033a0490bca0941
// (8 lines) for the idea;
// `memset_explicit` is CLEANROOM (C23) and does not copy the FreeBSD `memset_explicit.c`, whose
// SPDX line is misspelled and whose class is UNKNOWN (A-str-16). The barrier is the empty asm
// statement of the musl file, kept verbatim in shape.
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

//! Non-elidable zeroing: `explicit_bzero` and C23 `memset_explicit`.

/// Write a zero over every byte of `[p, p + n)` that the optimiser may not delete.
///
/// # Safety
///
/// `p` is writable for `n` bytes.
unsafe fn zero_with_barrier(p: *mut u8, n: usize) {
    // SAFETY: `memset`'s contract; the destination must not be optimised away, which the empty
    // asm below enforces.
    unsafe {
        super::mem::memset(p, 0, n);
        // The default `asm!` clobbers memory (there is no `nomem`), so the stores above are live
        // at this point even though the asm has no instructions.
        core::arch::asm!("/* {p} */", p = in(reg) p, options(nostack, preserves_flags));
    }
}

/// Zero `n` bytes at `p`, not removable by dead-store elimination.
///
/// # Safety
///
/// `p` is writable for `n` bytes.
#[inline]
pub unsafe fn explicit_bzero(p: *mut u8, n: usize) {
    // SAFETY: forwards the caller's writable-range contract.
    unsafe { zero_with_barrier(p, n) }
}

/// C23 `memset_explicit`: fill `n` bytes with `c`, not removable by dead-store elimination.
///
/// # Safety
///
/// `p` is writable for `n` bytes.
pub unsafe fn memset_explicit(p: *mut u8, c: i32, n: usize) {
    // SAFETY: `memset`'s contract; the same barrier keeps the stores live.
    unsafe {
        super::mem::memset(p, c, n);
        core::arch::asm!("/* {p} */", p = in(reg) p, options(nostack, preserves_flags));
    }
}
