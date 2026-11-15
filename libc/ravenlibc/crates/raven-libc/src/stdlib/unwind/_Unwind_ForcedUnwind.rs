#![cfg(target_os = "none")]
// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
use core::ffi::c_void;

use super::{Exception, ReasonCode, StopFn, forced_unwind_impl};

#[unsafe(no_mangle)]
pub unsafe extern "C" fn _Unwind_ForcedUnwind(
    exception: *mut Exception,
    stop: StopFn,
    argument: *mut c_void,
) -> ReasonCode {
    forced_unwind_impl(exception, stop, argument)
}
