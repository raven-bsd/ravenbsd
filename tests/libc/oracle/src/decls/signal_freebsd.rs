// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `signal-freebsd` oracle: `sig2str`/`str2sig` compiled with the BSD numbering shim.

#![allow(missing_docs)]
use core::ffi::{c_char, c_int};

#[cfg_attr(
    feature = "oracle-signal-freebsd",
    link(name = "oracle_signal_freebsd", kind = "dylib")
)]
unsafe extern "C" {
    pub fn oracle_sig2str(signum: c_int, buffer: *mut c_char) -> c_int;
    pub fn oracle_str2sig(text: *const c_char, number: *mut c_int) -> c_int;
}
