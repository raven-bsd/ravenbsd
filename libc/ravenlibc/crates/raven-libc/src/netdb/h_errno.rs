// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! The `h_errno` data symbol.
use core::ffi::c_int;

/// The legacy resolver error of the process (one cell until C57).
#[unsafe(no_mangle)]
pub static mut h_errno: c_int = 0;
