// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! The exported `optind` data symbol.
use core::ffi::c_int;

/// The index of the next `argv` element.
#[unsafe(no_mangle)]
pub static mut optind: c_int = 1;
