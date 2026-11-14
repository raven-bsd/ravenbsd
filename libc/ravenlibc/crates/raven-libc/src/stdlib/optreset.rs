// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! The exported `optreset` data symbol.
use core::ffi::c_int;

/// The BSD extension that restarts the `getopt` scan.
#[unsafe(no_mangle)]
pub static mut optreset: c_int = 0;
