// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//!
//! Minimal compiler-ABI unwind surface for Rust std.
//!
//! RavenBSD has no DWARF unwinder yet. The entry points exist so a std binary
//! links; the inspector functions return zero/NULL and `_Unwind_Backtrace`
//! returns `_URC_END_OF_STACK` without producing frames. `_Unwind_RaiseException`
//! and `_Unwind_ForcedUnwind` report the documented fatal phase codes, and
//! `_Unwind_Resume` aborts instead of pretending to resume. The target panic
//! strategy is `abort`, so none of these paths is reached by a normal build.
#![allow(non_snake_case, dead_code)]

use core::ffi::{c_int, c_void};

/// `_Unwind_Reason_Code`.
pub(crate) type ReasonCode = c_int;
/// The unwinder reached the end of the stack / no frames are available.
pub(crate) const URC_END_OF_STACK: ReasonCode = 5;
/// The search phase failed.
pub(crate) const URC_FATAL_PHASE1_ERROR: ReasonCode = 3;
/// The cleanup phase failed.
pub(crate) const URC_FATAL_PHASE2_ERROR: ReasonCode = 2;

/// Opaque `struct _Unwind_Context`.
#[repr(C)]
pub(crate) struct Context {
    _private: [u8; 0],
}

/// Opaque `struct _Unwind_Exception`.
#[repr(C)]
pub(crate) struct Exception {
    _private: [u8; 0],
}

/// `_Unwind_Trace_Fn`.
pub(crate) type TraceFn = Option<unsafe extern "C" fn(*mut Context, *mut c_void) -> ReasonCode>;
/// `_Unwind_Stop_Fn`.
pub(crate) type StopFn =
    Option<unsafe extern "C" fn(*mut Exception, ReasonCode, *mut c_void) -> ReasonCode>;

pub(crate) fn backtrace_impl(_trace: TraceFn, _argument: *mut c_void) -> ReasonCode {
    URC_END_OF_STACK
}

pub(crate) fn get_ip_impl(_context: *mut Context) -> usize {
    0
}

/// # Safety
/// `before` is NULL or writable for one `int`.
pub(crate) unsafe fn get_ip_info_impl(_context: *mut Context, before: *mut c_int) -> usize {
    if !before.is_null() {
        // SAFETY: the caller's writable int.
        unsafe { *before = 0 };
    }
    0
}

pub(crate) fn get_cfa_impl(_context: *mut Context) -> usize {
    0
}

pub(crate) fn get_gr_impl(_context: *mut Context, _index: c_int) -> usize {
    0
}

pub(crate) fn set_gr_impl(_context: *mut Context, _index: c_int, _value: usize) {}

pub(crate) fn get_language_specific_data_impl(_context: *mut Context) -> *mut c_void {
    core::ptr::null_mut()
}

pub(crate) fn get_region_start_impl(_context: *mut Context) -> usize {
    0
}

pub(crate) fn get_text_rel_base_impl(_context: *mut Context) -> usize {
    0
}

pub(crate) fn get_data_rel_base_impl(_context: *mut Context) -> usize {
    0
}

pub(crate) fn find_enclosing_function_impl(_pc: *mut c_void) -> *mut c_void {
    core::ptr::null_mut()
}

pub(crate) fn delete_exception_impl(_exception: *mut Exception) {}

pub(crate) fn raise_exception_impl(_exception: *mut Exception) -> ReasonCode {
    URC_FATAL_PHASE1_ERROR
}

#[cfg(target_os = "none")]
pub(crate) fn resume_impl(_exception: *mut Exception) -> ! {
    crate::stdlib::abort::abort()
}

pub(crate) fn forced_unwind_impl(
    _exception: *mut Exception,
    _stop: StopFn,
    _stop_argument: *mut c_void,
) -> ReasonCode {
    URC_FATAL_PHASE2_ERROR
}

pub(crate) fn set_ip_impl(_context: *mut Context, _ip: usize) {}

#[path = "_Unwind_Backtrace.rs"]
pub(crate) mod backtrace;
#[path = "_Unwind_DeleteException.rs"]
pub(crate) mod delete_exception;
#[path = "_Unwind_FindEnclosingFunction.rs"]
pub(crate) mod find_enclosing_function;
#[path = "_Unwind_ForcedUnwind.rs"]
pub(crate) mod forced_unwind;
#[path = "_Unwind_GetCFA.rs"]
pub(crate) mod get_cfa;
#[path = "_Unwind_GetDataRelBase.rs"]
pub(crate) mod get_data_rel_base;
#[path = "_Unwind_GetGR.rs"]
pub(crate) mod get_gr;
#[path = "_Unwind_GetIP.rs"]
pub(crate) mod get_ip;
#[path = "_Unwind_GetIPInfo.rs"]
pub(crate) mod get_ip_info;
#[path = "_Unwind_GetLanguageSpecificData.rs"]
pub(crate) mod get_language_specific_data;
#[path = "_Unwind_GetRegionStart.rs"]
pub(crate) mod get_region_start;
#[path = "_Unwind_GetTextRelBase.rs"]
pub(crate) mod get_text_rel_base;
#[path = "_Unwind_RaiseException.rs"]
pub(crate) mod raise_exception;
#[path = "_Unwind_Resume.rs"]
pub(crate) mod resume;
#[path = "_Unwind_SetGR.rs"]
pub(crate) mod set_gr;
#[path = "_Unwind_SetIP.rs"]
pub(crate) mod set_ip;

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects
)]
#[path = "mod_ut.rs"]
mod tests;
