// SPDX-License-Identifier: MIT
// Copyright (c) 2009-2016 by the contributors listed in CREDITS.TXT
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// RavenBSD: transpiled from rust:library/compiler-builtins/compiler-builtins/src/mem/impls.rs
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

//! Generic `memcpy`/`memmove`/`memset`/`memcmp` bodies (compiler-builtins adaptation).
//!
//! RavenBSD: `#[cfg(mem_unaligned)]` becomes `#[cfg(any(target_arch = "x86_64",
//! target_arch = "aarch64"))]` (there is no `build.rs`); `cold_path()` is an empty inline
//! helper; `debug_assert!` calls are removed because libc must have no panic edges (R-7);
//! every `wrapping_*` of the donor is kept (impls.rs line 9 note).

#![allow(clippy::arithmetic_side_effects)]
#![allow(clippy::style)]
#![allow(missing_docs)]
// The compiler-builtins unit is one audited unsafe implementation; every pointer operation is
// covered by the file contract (caller-provided, non-overlapping or explicitly overlapping
// ranges) rather than by per-expression blocks.
#![allow(unsafe_op_in_unsafe_fn)]

use core::ffi::c_int;

#[inline(always)]
fn cold_path() {}

const WORD_SIZE: usize = core::mem::size_of::<usize>();
const WORD_MASK: usize = WORD_SIZE - 1;

// max(2 * WORD_SIZE, 16) bytes before word-wise copying pays off.
const WORD_COPY_THRESHOLD: usize = if 2 * WORD_SIZE > 16 {
    2 * WORD_SIZE
} else {
    16
};

#[cfg(any(target_arch = "x86_64", target_arch = "aarch64"))]
unsafe fn read_usize_unaligned(x: *const usize) -> usize {
    // Do not use `core::ptr::read_unaligned` here, since it calls `copy_nonoverlapping` which
    // is translated to memcpy in LLVM.
    let x_read = (x as *const [u8; core::mem::size_of::<usize>()]).read();
    usize::from_ne_bytes(x_read)
}

/// Loads a `T`-sized chunk from `src` into `dst` at offset `offset`, if that does not exceed
/// `load_sz`. The offset pointers must both be `T`-aligned. Returns the new offset.
#[cfg(not(any(target_arch = "x86_64", target_arch = "aarch64")))]
#[inline(always)]
unsafe fn load_chunk_aligned<T: Copy>(
    src: *const usize,
    dst: *mut usize,
    load_sz: usize,
    offset: usize,
) -> usize {
    let chunk_sz = core::mem::size_of::<T>();
    if (load_sz & chunk_sz) != 0 {
        *dst.wrapping_byte_add(offset).cast::<T>() = *src.wrapping_byte_add(offset).cast::<T>();
        offset | chunk_sz
    } else {
        offset
    }
}

/// Load `load_sz` bytes from `src`, which must be `usize`-aligned, filling the out-of-bounds
/// part with 0s. `load_sz` is strictly less than `WORD_SIZE`.
#[cfg(not(any(target_arch = "x86_64", target_arch = "aarch64")))]
#[inline(always)]
unsafe fn load_aligned_partial(src: *const usize, load_sz: usize) -> usize {
    let mut i = 0;
    let mut out = 0usize;
    i = load_chunk_aligned::<u32>(src, &raw mut out, load_sz, i);
    i = load_chunk_aligned::<u16>(src, &raw mut out, load_sz, i);
    i = load_chunk_aligned::<u8>(src, &raw mut out, load_sz, i);
    let _ = i;
    out
}

/// Load `load_sz` bytes from the end of the aligned word, returning them as its last bytes.
#[cfg(not(any(target_arch = "x86_64", target_arch = "aarch64")))]
#[inline(always)]
unsafe fn load_aligned_end_partial(src: *const usize, load_sz: usize) -> usize {
    let mut i = 0;
    let mut out = 0usize;
    let src_shifted = src.wrapping_byte_add(WORD_SIZE - load_sz);
    let out_shifted = (&raw mut out).wrapping_byte_add(WORD_SIZE - load_sz);
    i = load_chunk_aligned::<u8>(src_shifted, out_shifted, load_sz, i);
    i = load_chunk_aligned::<u16>(src_shifted, out_shifted, load_sz, i);
    i = load_chunk_aligned::<u32>(src_shifted, out_shifted, load_sz, i);
    let _ = i;
    out
}

#[inline(always)]
pub unsafe fn copy_forward(mut dest: *mut u8, mut src: *const u8, mut n: usize) {
    #[inline(always)]
    unsafe fn copy_forward_bytes(mut dest: *mut u8, mut src: *const u8, n: usize) {
        let dest_end = dest.wrapping_add(n);
        while dest < dest_end {
            *dest = *src;
            dest = dest.wrapping_add(1);
            src = src.wrapping_add(1);
        }
    }

    #[inline(always)]
    unsafe fn copy_forward_aligned_words(dest: *mut u8, src: *const u8, n: usize) {
        let mut dest_usize = dest as *mut usize;
        let mut src_usize = src as *mut usize;
        let dest_end = dest.wrapping_add(n) as *mut usize;

        while dest_usize < dest_end {
            *dest_usize = *src_usize;
            dest_usize = dest_usize.wrapping_add(1);
            src_usize = src_usize.wrapping_add(1);
        }
    }

    /// `n` is in units of bytes, a multiple of the word size and not 0; `src` is not aligned.
    #[cfg(not(any(target_arch = "x86_64", target_arch = "aarch64")))]
    #[inline(always)]
    unsafe fn copy_forward_misaligned_words(dest: *mut u8, src: *const u8, n: usize) {
        let mut dest_usize = dest as *mut usize;
        let dest_end = dest.wrapping_add(n) as *mut usize;
        let offset = src as usize & WORD_MASK;
        let shift = offset * 8;
        let mut src_aligned = src.wrapping_byte_sub(offset) as *mut usize;
        let mut prev_word = load_aligned_end_partial(src_aligned, WORD_SIZE - offset);

        while dest_usize.wrapping_add(1) < dest_end {
            src_aligned = src_aligned.wrapping_add(1);
            let cur_word = *src_aligned;
            let reassembled = if cfg!(target_endian = "little") {
                prev_word >> shift | cur_word << (WORD_SIZE * 8 - shift)
            } else {
                prev_word << shift | cur_word >> (WORD_SIZE * 8 - shift)
            };
            prev_word = cur_word;
            *dest_usize = reassembled;
            dest_usize = dest_usize.wrapping_add(1);
        }

        src_aligned = src_aligned.wrapping_add(1);
        let cur_word = load_aligned_partial(src_aligned, offset);
        let reassembled = if cfg!(target_endian = "little") {
            prev_word >> shift | cur_word << (WORD_SIZE * 8 - shift)
        } else {
            prev_word << shift | cur_word >> (WORD_SIZE * 8 - shift)
        };
        *dest_usize = reassembled;
    }

    /// `n` is in units of bytes, a multiple of the word size and not 0; `src` is not aligned.
    #[cfg(any(target_arch = "x86_64", target_arch = "aarch64"))]
    #[inline(always)]
    unsafe fn copy_forward_misaligned_words(dest: *mut u8, src: *const u8, n: usize) {
        let mut dest_usize = dest as *mut usize;
        let mut src_usize = src as *mut usize;
        let dest_end = dest.wrapping_add(n) as *mut usize;

        while dest_usize < dest_end {
            *dest_usize = read_usize_unaligned(src_usize);
            dest_usize = dest_usize.wrapping_add(1);
            src_usize = src_usize.wrapping_add(1);
        }
    }

    if n >= WORD_COPY_THRESHOLD {
        let dest_misalignment = (dest as usize).wrapping_neg() & WORD_MASK;
        copy_forward_bytes(dest, src, dest_misalignment);
        dest = dest.wrapping_add(dest_misalignment);
        src = src.wrapping_add(dest_misalignment);
        n -= dest_misalignment;

        let n_words = n & !WORD_MASK;
        let src_misalignment = src as usize & WORD_MASK;
        if src_misalignment == 0 {
            copy_forward_aligned_words(dest, src, n_words);
        } else {
            cold_path();
            copy_forward_misaligned_words(dest, src, n_words);
        }
        dest = dest.wrapping_add(n_words);
        src = src.wrapping_add(n_words);
        n -= n_words;
    }
    copy_forward_bytes(dest, src, n);
}

#[inline(always)]
pub unsafe fn copy_backward(dest: *mut u8, src: *const u8, mut n: usize) {
    #[inline(always)]
    unsafe fn copy_backward_bytes(mut dest: *mut u8, mut src: *const u8, n: usize) {
        let dest_start = dest.wrapping_sub(n);
        while dest_start < dest {
            dest = dest.wrapping_sub(1);
            src = src.wrapping_sub(1);
            *dest = *src;
        }
    }

    #[inline(always)]
    unsafe fn copy_backward_aligned_words(dest: *mut u8, src: *const u8, n: usize) {
        let mut dest_usize = dest as *mut usize;
        let mut src_usize = src as *mut usize;
        let dest_start = dest.wrapping_sub(n) as *mut usize;

        while dest_start < dest_usize {
            dest_usize = dest_usize.wrapping_sub(1);
            src_usize = src_usize.wrapping_sub(1);
            *dest_usize = *src_usize;
        }
    }

    /// `n` is in units of bytes, a multiple of the word size and not 0; `src` is not aligned.
    #[cfg(not(any(target_arch = "x86_64", target_arch = "aarch64")))]
    #[inline(always)]
    unsafe fn copy_backward_misaligned_words(dest: *mut u8, src: *const u8, n: usize) {
        let mut dest_usize = dest as *mut usize;
        let dest_start = dest.wrapping_sub(n) as *mut usize;
        let offset = src as usize & WORD_MASK;
        let shift = offset * 8;
        let mut src_aligned = src.wrapping_byte_sub(offset) as *mut usize;
        let mut prev_word = load_aligned_partial(src_aligned, offset);

        while dest_start.wrapping_add(1) < dest_usize {
            src_aligned = src_aligned.wrapping_sub(1);
            let cur_word = *src_aligned;
            let reassembled = if cfg!(target_endian = "little") {
                prev_word << (WORD_SIZE * 8 - shift) | cur_word >> shift
            } else {
                prev_word >> (WORD_SIZE * 8 - shift) | cur_word << shift
            };
            prev_word = cur_word;
            dest_usize = dest_usize.wrapping_sub(1);
            *dest_usize = reassembled;
        }

        src_aligned = src_aligned.wrapping_sub(1);
        let cur_word = load_aligned_end_partial(src_aligned, WORD_SIZE - offset);
        let reassembled = if cfg!(target_endian = "little") {
            prev_word << (WORD_SIZE * 8 - shift) | cur_word >> shift
        } else {
            prev_word >> (WORD_SIZE * 8 - shift) | cur_word << shift
        };
        dest_usize = dest_usize.wrapping_sub(1);
        *dest_usize = reassembled;
    }

    /// `n` is in units of bytes, a multiple of the word size and not 0; `src` is not aligned.
    #[cfg(any(target_arch = "x86_64", target_arch = "aarch64"))]
    #[inline(always)]
    unsafe fn copy_backward_misaligned_words(dest: *mut u8, src: *const u8, n: usize) {
        let mut dest_usize = dest as *mut usize;
        let mut src_usize = src as *mut usize;
        let dest_start = dest.wrapping_sub(n) as *mut usize;

        while dest_start < dest_usize {
            dest_usize = dest_usize.wrapping_sub(1);
            src_usize = src_usize.wrapping_sub(1);
            *dest_usize = read_usize_unaligned(src_usize);
        }
    }

    let mut dest = dest.wrapping_add(n);
    let mut src = src.wrapping_add(n);

    if n >= WORD_COPY_THRESHOLD {
        let dest_misalignment = dest as usize & WORD_MASK;
        copy_backward_bytes(dest, src, dest_misalignment);
        dest = dest.wrapping_sub(dest_misalignment);
        src = src.wrapping_sub(dest_misalignment);
        n -= dest_misalignment;

        let n_words = n & !WORD_MASK;
        let src_misalignment = src as usize & WORD_MASK;
        if src_misalignment == 0 {
            copy_backward_aligned_words(dest, src, n_words);
        } else {
            cold_path();
            copy_backward_misaligned_words(dest, src, n_words);
        }
        dest = dest.wrapping_sub(n_words);
        src = src.wrapping_sub(n_words);
        n -= n_words;
    }
    copy_backward_bytes(dest, src, n);
}

#[inline(always)]
pub unsafe fn set_bytes(mut s: *mut u8, c: u8, mut n: usize) {
    #[inline(always)]
    pub unsafe fn set_bytes_bytes(mut s: *mut u8, c: u8, n: usize) {
        let end = s.wrapping_add(n);
        while s < end {
            *s = c;
            s = s.wrapping_add(1);
        }
    }

    #[inline(always)]
    pub unsafe fn set_bytes_words(s: *mut u8, c: u8, n: usize) {
        let mut broadcast = c as usize;
        let mut bits = 8;
        while bits < WORD_SIZE * 8 {
            broadcast |= broadcast << bits;
            bits *= 2;
        }

        let mut s_usize = s as *mut usize;
        let end = s.wrapping_add(n) as *mut usize;

        while s_usize < end {
            *s_usize = broadcast;
            s_usize = s_usize.wrapping_add(1);
        }
    }

    if n >= WORD_COPY_THRESHOLD {
        let misalignment = (s as usize).wrapping_neg() & WORD_MASK;
        set_bytes_bytes(s, c, misalignment);
        s = s.wrapping_add(misalignment);
        n -= misalignment;

        let n_words = n & !WORD_MASK;
        set_bytes_words(s, c, n_words);
        s = s.wrapping_add(n_words);
        n -= n_words;
    } else {
        cold_path();
    }
    set_bytes_bytes(s, c, n);
}

#[inline(always)]
pub unsafe fn compare_bytes(s1: *const u8, s2: *const u8, n: usize) -> c_int {
    let mut i = 0;
    while i < n {
        let a = *s1.wrapping_add(i);
        let b = *s2.wrapping_add(i);
        if a != b {
            return c_int::from(a) - c_int::from(b);
        }
        i += 1;
    }
    0
}

#[inline(always)]
pub unsafe fn c_string_length(mut s: *const core::ffi::c_char) -> usize {
    let mut n = 0;
    while *s != 0 {
        n += 1;
        s = s.wrapping_add(1);
    }
    n
}
