#![cfg(target_os = "none")]
// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
use super::{Exception, resume_impl};

#[unsafe(no_mangle)]
pub unsafe extern "C" fn _Unwind_Resume(exception: *mut Exception) -> ! {
    resume_impl(exception)
}
