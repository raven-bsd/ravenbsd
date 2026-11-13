// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `stdlib-musl` oracle: the rand48 family (CLEANROOM subject, musl as the differential oracle).

#![allow(missing_docs)] // extern declarations mirror the donor prototypes
use core::ffi::{c_double, c_long, c_ushort};

#[cfg_attr(
    feature = "oracle-stdlib-musl",
    link(name = "oracle_stdlib_musl", kind = "dylib")
)]
unsafe extern "C" {
    pub fn oracle_srand48(seed: c_long);
    pub fn oracle_seed48(seed16v: *mut c_ushort) -> *mut c_ushort;
    pub fn oracle_lcong48(p: *mut c_ushort);
    pub fn oracle_drand48() -> c_double;
    pub fn oracle_erand48(seed16v: *mut c_ushort) -> c_double;
    pub fn oracle_lrand48() -> c_long;
    pub fn oracle_nrand48(seed16v: *mut c_ushort) -> c_long;
    pub fn oracle_mrand48() -> c_long;
    pub fn oracle_jrand48(seed16v: *mut c_ushort) -> c_long;
}
