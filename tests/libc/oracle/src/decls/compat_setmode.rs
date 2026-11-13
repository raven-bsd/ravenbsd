// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! C declarations of the `compat-setmode` oracle (`openbsd:lib/libc/gen/setmode.c`).
//!
//! Wiring needs a C35 change (`decls/mod.rs` and the oracle feature list); the C65 crate carries
//! the 200 x 16 x 4 differential against the same donor instead (see `tests/data/setmode_oracle.rs`).

#![allow(dead_code)]

use core::ffi::{c_int, c_void};

unsafe extern "C" {
    pub fn oracle_setmode(spec: *const core::ffi::c_char) -> *mut c_void;
    pub fn oracle_getmode(bbox: *const c_void, omode: u32) -> u32;
    #[allow(dead_code)]
    pub static oracle_setmode_errno: c_int;
}
