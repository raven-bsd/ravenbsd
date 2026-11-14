// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `signal-musl` oracle: the eight donor sigset entry points.

#![allow(missing_docs)]
use core::ffi::c_int;

#[cfg_attr(
    feature = "oracle-signal-musl",
    link(name = "oracle_signal_musl", kind = "dylib")
)]
unsafe extern "C" {
    pub fn oracle_sigaddset(set: *mut u8, sig: c_int) -> c_int;
    pub fn oracle_sigdelset(set: *mut u8, sig: c_int) -> c_int;
    pub fn oracle_sigemptyset(set: *mut u8) -> c_int;
    pub fn oracle_sigfillset(set: *mut u8) -> c_int;
    pub fn oracle_sigismember(set: *const u8, sig: c_int) -> c_int;
    pub fn oracle_sigisemptyset(set: *const u8) -> c_int;
    pub fn oracle_sigandset(dest: *mut u8, left: *const u8, right: *const u8) -> c_int;
    pub fn oracle_sigorset(dest: *mut u8, left: *const u8, right: *const u8) -> c_int;
}
