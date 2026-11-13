// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! C declarations of the `compat-vis` oracle (`netbsd:lib/libc/gen/{vis,unvis}.c`).
//!
//! The recipe builds them with `--prefix-symbols=oracle_`; the differential test would call
//! these through the shared object. C65 does not own `decls/mod.rs` or the oracle Cargo.toml
//! feature list, so wiring them is a C35 change (the report records the deferral).

#![allow(dead_code)]

use core::ffi::{c_char, c_int, c_uint};

unsafe extern "C" {
    pub fn oracle_vis(dst: *mut c_char, c: c_int, flags: c_int, nextc: c_int) -> *mut c_char;
    pub fn oracle_nvis(dst: *mut c_char, dlen: usize, c: c_int, flags: c_int, nextc: c_int) -> *mut c_char;
    pub fn oracle_svis(dst: *mut c_char, c: c_int, flags: c_int, nextc: c_int, extra: *const c_char) -> *mut c_char;
    pub fn oracle_snvis(dst: *mut c_char, dlen: usize, c: c_int, flags: c_int, nextc: c_int, extra: *const c_char) -> *mut c_char;
    pub fn oracle_strvis(dst: *mut c_char, src: *const c_char, flags: c_int) -> c_int;
    pub fn oracle_strsvis(dst: *mut c_char, src: *const c_char, flags: c_int, extra: *const c_char) -> c_int;
    pub fn oracle_strnvis(dst: *mut c_char, dlen: usize, src: *const c_char, flags: c_int) -> c_int;
    pub fn oracle_strsnvis(dst: *mut c_char, dlen: usize, src: *const c_char, flags: c_int, extra: *const c_char) -> c_int;
    pub fn oracle_strvisx(dst: *mut c_char, src: *const c_char, len: usize, flags: c_int) -> c_int;
    pub fn oracle_strnvisx(dst: *mut c_char, dlen: usize, src: *const c_char, len: usize, flags: c_int) -> c_int;
    pub fn oracle_strsvisx(dst: *mut c_char, src: *const c_char, len: usize, flags: c_int, extra: *const c_char) -> c_int;
    pub fn oracle_strsnvisx(dst: *mut c_char, dlen: usize, src: *const c_char, len: usize, flags: c_int, extra: *const c_char) -> c_int;
    pub fn oracle_unvis(cp: *mut c_char, c: c_int, astate: *mut c_int, flag: c_int) -> c_int;
    pub fn oracle_strunvis(dst: *mut c_char, src: *const c_char) -> c_int;
    pub fn oracle_strnunvis(dst: *mut c_char, dlen: usize, src: *const c_char) -> c_int;
    pub fn oracle_strunvisx(dst: *mut c_char, src: *const c_char, flags: c_int) -> c_int;
    pub fn oracle_strnunvisx(dst: *mut c_char, dlen: usize, src: *const c_char, flags: c_int) -> c_int;
    pub fn oracle_strenvisx(dst: *mut c_char, dlen: usize, src: *const c_char, len: usize, flags: c_int, cerr: *mut c_int) -> c_int;
    #[allow(dead_code)]
    pub static oracle_vis_end: c_uint;
}
