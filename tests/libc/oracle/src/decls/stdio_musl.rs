// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `stdio-musl` oracle: the musl FILE core built against `shim/stdio/musl`.
//!
//! The hook table is the only data symbol the harness writes; the Rust replacement calls
//! through it so the C original and the Rust subject share one in-memory file system.

#![allow(missing_docs)] // extern declarations mirror the donor prototypes

use core::ffi::{c_char, c_int, c_long, c_void};

use crate::support_stdio::Hooks;

#[cfg_attr(
    feature = "oracle-stdio-musl",
    link(name = "oracle_stdio_musl", kind = "dylib")
)]
unsafe extern "C" {
    pub static mut oracle_rl_stdio_hooks: Hooks;

    pub fn oracle_fopen(path: *const c_char, mode: *const c_char) -> *mut c_void;
    pub fn oracle_fclose(file: *mut c_void) -> c_int;
    pub fn oracle_fflush(file: *mut c_void) -> c_int;
    pub fn oracle_fread(ptr: *mut c_void, size: usize, nmemb: usize, file: *mut c_void) -> usize;
    pub fn oracle_fwrite(ptr: *const c_void, size: usize, nmemb: usize, file: *mut c_void)
    -> usize;
    pub fn oracle_fgetc(file: *mut c_void) -> c_int;
    pub fn oracle_fputc(c: c_int, file: *mut c_void) -> c_int;
    pub fn oracle_fgets(buf: *mut c_char, n: c_int, file: *mut c_void) -> *mut c_char;
    pub fn oracle_fputs(text: *const c_char, file: *mut c_void) -> c_int;
    pub fn oracle_ungetc(c: c_int, file: *mut c_void) -> c_int;
    pub fn oracle_fseek(file: *mut c_void, off: c_long, whence: c_int) -> c_int;
    pub fn oracle_fseeko(file: *mut c_void, off: i64, whence: c_int) -> c_int;
    pub fn oracle_ftell(file: *mut c_void) -> c_long;
    pub fn oracle_ftello(file: *mut c_void) -> i64;
    pub fn oracle_rewind(file: *mut c_void);
    pub fn oracle_clearerr(file: *mut c_void);
    pub fn oracle_feof(file: *mut c_void) -> c_int;
    pub fn oracle_ferror(file: *mut c_void) -> c_int;
    pub fn oracle_fileno(file: *mut c_void) -> c_int;
    pub fn oracle_setvbuf(file: *mut c_void, buf: *mut c_char, kind: c_int, size: usize) -> c_int;
    pub fn oracle_fmemopen(buf: *mut c_void, size: usize, mode: *const c_char) -> *mut c_void;
    pub fn oracle_open_memstream(bufp: *mut *mut c_char, sizep: *mut usize) -> *mut c_void;
    pub fn oracle_getdelim(
        lineptr: *mut *mut c_char,
        n: *mut usize,
        delim: c_int,
        file: *mut c_void,
    ) -> isize;
    pub fn oracle_getline(lineptr: *mut *mut c_char, n: *mut usize, file: *mut c_void) -> isize;
    pub fn oracle_fgetpos(file: *mut c_void, pos: *mut i64) -> c_int;
    pub fn oracle_fsetpos(file: *mut c_void, pos: *const i64) -> c_int;
}

unsafe extern "C" {
    pub fn free(ptr: *mut c_void);
    pub fn malloc(size: usize) -> *mut c_void;
}
