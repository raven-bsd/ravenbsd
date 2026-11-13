// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `string-basic` oracle: `strlcpy` and `strnlen` (OpenBSD, ISC) and `memchr` (musl, MIT via
//! `musl/COPYRIGHT`), built by `libc-oracle build string-basic` and renamed `oracle_*`.

use core::ffi::{c_char, c_int, c_void};

#[cfg_attr(
    feature = "oracle-string-basic",
    link(name = "oracle_string_basic", kind = "dylib")
)]
unsafe extern "C" {
    /// Donor `strlcpy(char *dst, const char *src, size_t dsize)`.
    pub fn oracle_strlcpy(dst: *mut c_char, src: *const c_char, dsize: usize) -> usize;
    /// Donor `strnlen(const char *str, size_t maxlen)`.
    pub fn oracle_strnlen(str: *const c_char, maxlen: usize) -> usize;
    /// Donor `memchr(const void *src, int c, size_t n)`.
    pub fn oracle_memchr(src: *const c_void, c: c_int, n: usize) -> *mut c_void;
}
