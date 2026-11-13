// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `fmt-musl-scanf` oracle: musl's `sscanf` and `swscanf` entries.
//!
//! The glibc `<wchar.h>` redirects `swscanf` to `__isoc99_swscanf` when the
//! donor is compiled, so the exported symbol is `oracle___isoc99_swscanf`.

#![allow(missing_docs)] // extern declarations mirror the donor prototypes
use core::ffi::{c_char, c_int};

#[cfg_attr(
    feature = "oracle-fmt-musl-scanf",
    link(name = "oracle_fmt_musl_scanf", kind = "dylib")
)]
unsafe extern "C" {
    pub fn oracle_sscanf(s: *const c_char, fmt: *const c_char, ...) -> c_int;
    pub fn oracle___isoc99_swscanf(s: *const i32, fmt: *const i32, ...) -> c_int;
}

// Keep the host libm in the executable's DT_NEEDED list so the musl scanf
// object's `fmodl`/`fabsl`/`copysignl`/`scalbnl` references resolve at load.
//
// Only the addresses are taken: these functions return C `long double` in the
// x87 stack, a type stable Rust cannot name, and calling them through an `f64`
// declaration would leave the x87 stack unbalanced.
#[link(name = "m")]
unsafe extern "C" {
    fn fmodl();
    fn fabsl();
    fn copysignl();
    fn scalbnl();
}

/// Touch every libm symbol the donor needs without calling it.
pub fn libm_probe() {
    let keep = fmodl as *const () as usize
        ^ fabsl as *const () as usize
        ^ copysignl as *const () as usize
        ^ scalbnl as *const () as usize;
    std::hint::black_box(keep);
}
