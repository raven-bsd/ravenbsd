// SPDX-License-Identifier: BSD-2-Clause
// Copyright (c) 2025 Robert Clausecker <fuz@FreeBSD.org>
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// RavenBSD: transpiled from freebsd:lib/libc/stdbit/stdc_{leading_zeros,leading_ones,
// trailing_zeros,trailing_ones,first_leading_zero,first_leading_one,first_trailing_zero,
// first_trailing_one,count_zeros,count_ones,has_single_bit,bit_width,bit_floor,bit_ceil}.c
// @ 133344405f532bd54ff3629a77d4c4e062f01fdb (all 14 files carry this same BSD-2 header).
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

//! C23 `<stdbit.h>`: 14 families x 5 widths.
//!
//! Every function counts **within its own width** (`u8::leading_zeros` is 0..=8, not 0..=32), so
//! the donor's `UINT_WIDTH - UCHAR_WIDTH` fix-ups are unnecessary (A-str-11). `ul` and `ull` are
//! both 64-bit in the Raven ABI.
//!
//! RavenBSD: the shift arithmetic is bounded by the type width; the lint allow records it.

#![allow(clippy::arithmetic_side_effects)]

macro_rules! stdbit {
    (
        $ty:ty,
        $leading_zeros:ident, $leading_ones:ident, $trailing_zeros:ident, $trailing_ones:ident,
        $first_leading_zero:ident, $first_leading_one:ident,
        $first_trailing_zero:ident, $first_trailing_one:ident,
        $count_zeros:ident, $count_ones:ident,
        $has_single_bit:ident, $bit_width:ident, $bit_floor:ident, $bit_ceil:ident
    ) => {
        #[doc = concat!("C23 `stdc_leading_zeros` for `", stringify!($ty), "`.")]
        #[must_use]
        pub fn $leading_zeros(x: $ty) -> u32 {
            x.leading_zeros()
        }

        #[doc = concat!("C23 `stdc_leading_ones` for `", stringify!($ty), "`.")]
        #[must_use]
        pub fn $leading_ones(x: $ty) -> u32 {
            (!x).leading_zeros()
        }

        #[doc = concat!("C23 `stdc_trailing_zeros` for `", stringify!($ty), "`.")]
        #[must_use]
        pub fn $trailing_zeros(x: $ty) -> u32 {
            x.trailing_zeros()
        }

        #[doc = concat!("C23 `stdc_trailing_ones` for `", stringify!($ty), "`.")]
        #[must_use]
        pub fn $trailing_ones(x: $ty) -> u32 {
            (!x).trailing_zeros()
        }

        #[doc = concat!("C23 `stdc_first_leading_zero` for `", stringify!($ty), "`.")]
        #[must_use]
        pub fn $first_leading_zero(x: $ty) -> u32 {
            if x == <$ty>::MAX {
                0
            } else {
                (!x).leading_zeros() + 1
            }
        }

        #[doc = concat!("C23 `stdc_first_leading_one` for `", stringify!($ty), "`.")]
        #[must_use]
        pub fn $first_leading_one(x: $ty) -> u32 {
            if x == 0 { 0 } else { x.leading_zeros() + 1 }
        }

        #[doc = concat!("C23 `stdc_first_trailing_zero` for `", stringify!($ty), "`.")]
        #[must_use]
        pub fn $first_trailing_zero(x: $ty) -> u32 {
            if x == <$ty>::MAX {
                0
            } else {
                (!x).trailing_zeros() + 1
            }
        }

        #[doc = concat!("C23 `stdc_first_trailing_one` for `", stringify!($ty), "`.")]
        #[must_use]
        pub fn $first_trailing_one(x: $ty) -> u32 {
            if x == 0 { 0 } else { x.trailing_zeros() + 1 }
        }

        #[doc = concat!("C23 `stdc_count_zeros` for `", stringify!($ty), "`.")]
        #[must_use]
        pub fn $count_zeros(x: $ty) -> u32 {
            (!x).count_ones()
        }

        #[doc = concat!("C23 `stdc_count_ones` for `", stringify!($ty), "`.")]
        #[must_use]
        pub fn $count_ones(x: $ty) -> u32 {
            x.count_ones()
        }

        #[doc = concat!("C23 `stdc_has_single_bit` for `", stringify!($ty), "`.")]
        #[must_use]
        pub fn $has_single_bit(x: $ty) -> bool {
            x.is_power_of_two()
        }

        #[doc = concat!("C23 `stdc_bit_width` for `", stringify!($ty), "`.")]
        #[must_use]
        pub fn $bit_width(x: $ty) -> u32 {
            if x == 0 {
                0
            } else {
                <$ty>::BITS - x.leading_zeros()
            }
        }

        #[doc = concat!("C23 `stdc_bit_floor` for `", stringify!($ty), "`.")]
        #[must_use]
        pub fn $bit_floor(x: $ty) -> $ty {
            if x == 0 {
                0
            } else {
                (1 as $ty) << (<$ty>::BITS - 1 - x.leading_zeros())
            }
        }

        #[doc = concat!("C23 `stdc_bit_ceil` for `", stringify!($ty), "`.")]
        #[must_use]
        pub fn $bit_ceil(x: $ty) -> $ty {
            if x <= 1 {
                1
            } else if x > <$ty>::MAX / 2 + 1 {
                0
            } else {
                (1 as $ty) << (<$ty>::BITS - (x - 1).leading_zeros())
            }
        }
    };
}

stdbit!(
    u8,
    stdc_leading_zeros_uc,
    stdc_leading_ones_uc,
    stdc_trailing_zeros_uc,
    stdc_trailing_ones_uc,
    stdc_first_leading_zero_uc,
    stdc_first_leading_one_uc,
    stdc_first_trailing_zero_uc,
    stdc_first_trailing_one_uc,
    stdc_count_zeros_uc,
    stdc_count_ones_uc,
    stdc_has_single_bit_uc,
    stdc_bit_width_uc,
    stdc_bit_floor_uc,
    stdc_bit_ceil_uc
);

stdbit!(
    u16,
    stdc_leading_zeros_us,
    stdc_leading_ones_us,
    stdc_trailing_zeros_us,
    stdc_trailing_ones_us,
    stdc_first_leading_zero_us,
    stdc_first_leading_one_us,
    stdc_first_trailing_zero_us,
    stdc_first_trailing_one_us,
    stdc_count_zeros_us,
    stdc_count_ones_us,
    stdc_has_single_bit_us,
    stdc_bit_width_us,
    stdc_bit_floor_us,
    stdc_bit_ceil_us
);

stdbit!(
    u32,
    stdc_leading_zeros_ui,
    stdc_leading_ones_ui,
    stdc_trailing_zeros_ui,
    stdc_trailing_ones_ui,
    stdc_first_leading_zero_ui,
    stdc_first_leading_one_ui,
    stdc_first_trailing_zero_ui,
    stdc_first_trailing_one_ui,
    stdc_count_zeros_ui,
    stdc_count_ones_ui,
    stdc_has_single_bit_ui,
    stdc_bit_width_ui,
    stdc_bit_floor_ui,
    stdc_bit_ceil_ui
);

stdbit!(
    u64,
    stdc_leading_zeros_ul,
    stdc_leading_ones_ul,
    stdc_trailing_zeros_ul,
    stdc_trailing_ones_ul,
    stdc_first_leading_zero_ul,
    stdc_first_leading_one_ul,
    stdc_first_trailing_zero_ul,
    stdc_first_trailing_one_ul,
    stdc_count_zeros_ul,
    stdc_count_ones_ul,
    stdc_has_single_bit_ul,
    stdc_bit_width_ul,
    stdc_bit_floor_ul,
    stdc_bit_ceil_ul
);

stdbit!(
    u64,
    stdc_leading_zeros_ull,
    stdc_leading_ones_ull,
    stdc_trailing_zeros_ull,
    stdc_trailing_ones_ull,
    stdc_first_leading_zero_ull,
    stdc_first_leading_one_ull,
    stdc_first_trailing_zero_ull,
    stdc_first_trailing_one_ull,
    stdc_count_zeros_ull,
    stdc_count_ones_ull,
    stdc_has_single_bit_ull,
    stdc_bit_width_ull,
    stdc_bit_floor_ull,
    stdc_bit_ceil_ull
);

#[cfg(test)]
#[path = "stdbit_ut.rs"]
mod tests;
