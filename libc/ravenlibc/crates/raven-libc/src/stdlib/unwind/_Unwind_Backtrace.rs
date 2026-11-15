#![cfg(target_os = "none")]
// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
use core::ffi::c_void;

use super::{ReasonCode, TraceFn, backtrace_impl};

#[unsafe(no_mangle)]
pub unsafe extern "C" fn _Unwind_Backtrace(trace: TraceFn, argument: *mut c_void) -> ReasonCode {
    backtrace_impl(trace, argument)
}
