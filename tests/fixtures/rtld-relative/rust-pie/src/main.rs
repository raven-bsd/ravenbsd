// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! C74o regression: a dynamic Rust PIE with `PT_INTERP` and `DT_NEEDED libc.so.1`.
//!
//! The binary uses `raven_rt`'s `_start`/`__raven_main` and does **not** export a C `main`.
//! Before C74o, `libc.so.1` carried a strong undefined `main` and the loader refused to load
//! it (`non-plt: Undefined(2)`). The `getpid` call forces a real `DT_NEEDED libc.so.1`, so the
//! loader must resolve libc even though this entry is not the C crt start.

#![no_std]
#![no_main]

use core::hint::black_box;
use raven_rt::entry;
use raven_rt::println;

entry!(main);

static TARGET: u32 = 7;
static POINTER: &u32 = &TARGET;

unsafe extern "C" {
    /// A libc-only symbol; the linker must add `DT_NEEDED libc.so.1`.
    fn getpid() -> i32;
}

fn main(_args: raven_rt::Args) -> i32 {
    // SAFETY: `getpid` is the C ABI entry with no arguments.
    let pid = unsafe { getpid() };
    let target = black_box(*POINTER);
    println!("rtld-relative-rust: main pid={} relative={}", black_box(pid), target);
    if target == 7 { 0 } else { 1 }
}
