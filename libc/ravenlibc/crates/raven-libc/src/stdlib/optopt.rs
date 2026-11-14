// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! The exported `optopt` data symbol.
use core::ffi::c_int;

/// The offending option character of the last failure.
#[unsafe(no_mangle)]
pub static mut optopt: c_int = 0;
