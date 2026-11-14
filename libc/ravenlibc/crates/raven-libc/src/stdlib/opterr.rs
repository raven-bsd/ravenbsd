// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! The exported `opterr` data symbol.
use core::ffi::c_int;

/// Whether `getopt` prints diagnostics.
#[unsafe(no_mangle)]
pub static mut opterr: c_int = 1;
