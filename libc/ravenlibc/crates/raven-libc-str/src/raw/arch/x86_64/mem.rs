// SPDX-License-Identifier: MIT
// Copyright (c) 2009-2016 by the contributors listed in CREDITS.TXT
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// RavenBSD: transpiled from rust:library/compiler-builtins/compiler-builtins/src/mem/x86_64.rs
// @ 8bab26f4f68e0e26f0bb7960be334d5b520ea452
// (no per-file header; the crate licence is MIT AND Apache-2.0 WITH LLVM-exception AND
//  (MIT OR Apache-2.0) at compiler-builtins/Cargo.toml:18; MIT elected. The MIT text of
//  library/compiler-builtins/LICENSE.txt is reproduced below.)
//
// MIT License
//
// Permission is hereby granted, free of charge, to any person obtaining a copy
// of this software and associated documentation files (the "Software"), to deal
// in the Software without restriction, including without limitation the rights
// to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
// copies of the Software, and to permit persons to whom the Software is
// furnished to do so, subject to the following conditions:
//
// The above copyright notice and this permission notice shall be included in all
// copies or substantial portions of the Software.
//
// THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
// IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
// FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
// AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
// LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
// OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
// SOFTWARE.

//! x86_64 `rep`-prefix block operations (compiler-builtins adaptation).
//!
//! RavenBSD: `core::intrinsics::unchecked_div/unchecked_rem` (nightly) are replaced by `/` and
//! `%` on the constant `size_of::<T>()`; the `cld` after the backward copy is kept; `rep movsb`
//! is used only when `target_feature = "ermsb"` and only for forward copies (A-str-14); the SSE2
//! `c_string_length` is gated on `sse2`, so a soft-float build uses the generic word loop.

#![allow(clippy::arithmetic_side_effects)]
#![allow(clippy::style)]
#![allow(missing_docs)]
#![allow(unsafe_op_in_unsafe_fn)]

use core::arch::asm;
use core::mem;

#[inline(always)]
#[cfg(target_feature = "ermsb")]
pub unsafe fn copy_forward(dest: *mut u8, src: *const u8, count: usize) {
    asm!(
        "rep movsb [rdi], [rsi]",
        inout("rcx") count => _,
        inout("rdi") dest => _,
        inout("rsi") src => _,
        options(nostack, preserves_flags)
    );
}

#[inline(always)]
#[cfg(not(target_feature = "ermsb"))]
pub unsafe fn copy_forward(mut dest: *mut u8, mut src: *const u8, count: usize) {
    let (pre_byte_count, qword_count, byte_count) = rep_param(dest, count);
    asm!(
        "rep movsb",
        inout("ecx") pre_byte_count => _,
        inout("rdi") dest => dest,
        inout("rsi") src => src,
        options(nostack, preserves_flags)
    );
    asm!(
        "rep movsq",
        inout("rcx") qword_count => _,
        inout("rdi") dest => dest,
        inout("rsi") src => src,
        options(nostack, preserves_flags)
    );
    asm!(
        "rep movsb",
        inout("ecx") byte_count => _,
        inout("rdi") dest => _,
        inout("rsi") src => _,
        options(nostack, preserves_flags)
    );
}

#[inline(always)]
pub unsafe fn copy_backward(dest: *mut u8, src: *const u8, count: usize) {
    let (pre_byte_count, qword_count, byte_count) = rep_param(dest, count);
    // The blocks cannot be separated because of the std/cld pair.
    asm!(
        "std",
        "rep movsb",
        "sub rsi, 7",
        "sub rdi, 7",
        "mov rcx, {qword_count:r}",
        "rep movsq",
        "add rsi, 7",
        "add rdi, 7",
        "mov ecx, {pre_byte_count:e}",
        "rep movsb",
        "cld",
        pre_byte_count = in(reg) pre_byte_count,
        qword_count = in(reg) qword_count,
        inout("ecx") byte_count => _,
        inout("rdi") dest.add(count - 1) => _,
        inout("rsi") src.add(count - 1) => _,
        options(nostack)
    );
}

#[inline(always)]
#[cfg(target_feature = "ermsb")]
pub unsafe fn set_bytes(dest: *mut u8, c: u8, count: usize) {
    asm!(
        "rep stosb [rdi], al",
        inout("rcx") count => _,
        inout("rdi") dest => _,
        inout("al") c => _,
        options(nostack, preserves_flags)
    )
}

#[inline(always)]
#[cfg(not(target_feature = "ermsb"))]
pub unsafe fn set_bytes(mut dest: *mut u8, c: u8, count: usize) {
    let c = c as u64 * 0x0101_0101_0101_0101;
    let (pre_byte_count, qword_count, byte_count) = rep_param(dest, count);
    asm!(
        "rep stosb",
        inout("ecx") pre_byte_count => _,
        inout("rdi") dest => dest,
        in("rax") c,
        options(nostack, preserves_flags)
    );
    asm!(
        "rep stosq",
        inout("rcx") qword_count => _,
        inout("rdi") dest => dest,
        in("rax") c,
        options(nostack, preserves_flags)
    );
    asm!(
        "rep stosb",
        inout("ecx") byte_count => _,
        inout("rdi") dest => _,
        in("rax") c,
        options(nostack, preserves_flags)
    );
}

#[inline(always)]
pub unsafe fn compare_bytes(a: *const u8, b: *const u8, n: usize) -> i32 {
    #[inline(always)]
    unsafe fn cmp<T, U, F>(mut a: *const T, mut b: *const T, n: usize, f: F) -> i32
    where
        T: Clone + Copy + Eq,
        U: Clone + Copy + Eq,
        F: FnOnce(*const U, *const U, usize) -> i32,
    {
        const { assert!(mem::size_of::<T>() != 0) };
        let size = mem::size_of::<T>();
        let end = a.add(n / size);
        while a != end {
            if a.read_unaligned() != b.read_unaligned() {
                return f(a.cast(), b.cast(), size);
            }
            a = a.add(1);
            b = b.add(1);
        }
        f(a.cast(), b.cast(), n % size)
    }
    let c1 = |mut a: *const u8, mut b: *const u8, n| {
        for _ in 0..n {
            if a.read() != b.read() {
                return i32::from(a.read()) - i32::from(b.read());
            }
            a = a.add(1);
            b = b.add(1);
        }
        0
    };
    let c2 = |a: *const u16, b, n| cmp(a, b, n, c1);
    let c4 = |a: *const u32, b, n| cmp(a, b, n, c2);
    let c8 = |a: *const u64, b, n| cmp(a, b, n, c4);
    let c16 = |a: *const u128, b, n| cmp(a, b, n, c8);
    c16(a.cast(), b.cast(), n)
}

#[cfg(target_feature = "sse2")]
#[inline(always)]
pub unsafe fn c_string_length(mut s: *const core::ffi::c_char) -> usize {
    use core::arch::x86_64::{__m128i, _mm_cmpeq_epi8, _mm_movemask_epi8, _mm_set1_epi8};

    let mut n = 0;

    for _ in 0..4 {
        if *s == 0 {
            return n;
        }
        n += 1;
        s = s.add(1);
    }

    let align = s as usize & 15;
    let mut s = ((s as usize) - align) as *const __m128i;
    let zero = _mm_set1_epi8(0);

    let x = {
        let r;
        asm!(
            "movdqa {dest}, [{addr:r}]",
            addr = in(reg) s,
            dest = out(xmm_reg) r,
            options(nostack, preserves_flags),
        );
        r
    };
    let cmp = _mm_movemask_epi8(_mm_cmpeq_epi8(x, zero)) >> align;

    if cmp != 0 {
        return n + cmp.trailing_zeros() as usize;
    }

    n += 16 - align;
    s = s.add(1);

    loop {
        let x = {
            let r;
            asm!(
                "movdqa {dest}, [{addr:r}]",
                addr = in(reg) s,
                dest = out(xmm_reg) r,
                options(nostack, preserves_flags),
            );
            r
        };
        let cmp = _mm_movemask_epi8(_mm_cmpeq_epi8(x, zero)) as u32;
        if cmp == 0 {
            n += 16;
            s = s.add(1);
        } else {
            return n + cmp.trailing_zeros() as usize;
        }
    }
}

#[cfg(not(target_feature = "sse2"))]
#[inline(always)]
pub unsafe fn c_string_length(mut s: *const core::ffi::c_char) -> usize {
    let mut n = 0;

    while s as usize & 7 != 0 {
        if *s == 0 {
            return n;
        }
        n += 1;
        s = s.add(1);
    }

    let mut s = s as *const u64;

    loop {
        let mut cs = {
            let r: u64;
            asm!(
                "mov {dest}, [{addr}]",
                addr = in(reg) s,
                dest = out(reg) r,
                options(nostack, preserves_flags),
            );
            r
        };
        if (cs.wrapping_sub(0x0101010101010101) & !cs & 0x8080808080808080) != 0 {
            loop {
                if cs & 255 == 0 {
                    return n;
                } else {
                    cs >>= 8;
                    n += 1;
                }
            }
        } else {
            n += 8;
            s = s.add(1);
        }
    }
}

/// Determine optimal parameters for a `rep` instruction.
fn rep_param(dest: *mut u8, mut count: usize) -> (usize, usize, usize) {
    let pre_byte_count = ((8 - (dest as usize & 0b111)) & 0b111).min(count);
    count -= pre_byte_count;
    let qword_count = count >> 3;
    let byte_count = count & 0b111;
    (pre_byte_count, qword_count, byte_count)
}
