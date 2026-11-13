// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// RavenBSD: C-ABI shell for the updwtmpx export (ticket C65, `17` section 13). The logic is in
// `raven-libc-compat`; this file owns the C struct, the caller buffers and the errno write.
//! `updwtmpx` C-ABI export (ticket C65).

use core::ffi::{c_char, c_int};

/// `updwtmpx`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn updwtmpx(_file: *const c_char, _line: *const core::ffi::c_void) {}
