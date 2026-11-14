// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! The exported `environ` data symbol.
use core::ffi::c_char;

/// The process environment pointer array (C61's start writes the kernel `envp` here).
#[unsafe(no_mangle)]
pub static mut environ: *mut *mut c_char = core::ptr::null_mut();
