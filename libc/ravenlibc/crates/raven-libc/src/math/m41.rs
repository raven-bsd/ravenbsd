// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The classification and comparison macros as function symbols.
//!
//! `<math.h>` defines the type-generic macros; these exported functions are the double forms
//! that C89/C99 code and FFI users can name directly.

fn isfinite_value(x: f64) -> i32 {
    i32::from(raven_libc_m::isfinite(x))
}

fn isnan_value(x: f64) -> i32 {
    i32::from(raven_libc_m::isnan(x))
}

fn isnormal_value(x: f64) -> i32 {
    i32::from(raven_libc_m::isnormal(x))
}

unary_i32!(fpclassify, raven_libc_m::fpclassify);
unary_i32!(isfinite, isfinite_value);
unary_i32!(isnan, isnan_value);
unary_i32!(isnormal, isnormal_value);
