// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//!
//! `printf(3)` and its `vprintf` target, plus the float and wide weak hooks of the caller file.
//!
//! The engine is C43's `vformat`; the argument reader is C59's [`crate::va::CEnv`]; the FILE side
//! is C56's glue. The float hook is the M2 plug-in: its dummy prints `*float*` (the engine does
//! that on [`raven_libc_fmt::FmtErr::FloatUnavailable`]) and its strong member supplies C42
//! digits.

use core::ffi::{c_char, c_int};

use raven_libc_fmt::{DigitsOut, FloatBits, FloatFmt, FmtErr, FmtOptions};
use raven_libc_sys::weak_hook;

use crate::stdio::va::{List, reader};

/// The `%e`/`%f` digit dummy: fail, so the engine prints `*float*`.
fn dummy_float(_value: FloatBits, _mode: u8, _ndigits: i32, _out: &mut [u8]) -> Option<DigitsOut> {
    None
}

weak_hook! {
    /// The float-digit plug-in (`donor-map/16` R-13, mechanism M2).
    pub(crate) fn __rl_fmt_float(
        value: raven_libc_fmt::FloatBits,
        mode: u8,
        ndigits: i32,
        out: &mut [u8],
    ) -> Option<raven_libc_fmt::DigitsOut>;
    dummy = crate::stdio::vprintf::dummy_float;
}

/// The `%lc`/`%ls` byte conversion dummy: skip the argument.
#[allow(dead_code)] // the macro's dummy is only called when the hook cell is used
fn dummy_wide(_wc: u32, _out: &mut [u8; 4]) -> usize {
    0
}

weak_hook! {
    /// The wide-character plug-in; the dummy skips the argument.
    pub(crate) fn __rl_fmt_wide(wc: u32, out: &mut [u8; 4]) -> usize;
    dummy = crate::stdio::vprintf::dummy_wide;
}

/// The float plug-in over the weak hook.
pub(crate) struct FloatHook;

impl FloatFmt for FloatHook {
    fn dec_digits(
        &self,
        value: FloatBits,
        mode: u8,
        ndigits: i32,
        out: &mut [u8],
    ) -> Result<DigitsOut, FmtErr> {
        match __rl_fmt_float(value, mode, ndigits, out) {
            Some(digits) => Ok(digits),
            None => Err(FmtErr::FloatUnavailable),
        }
    }

    fn hex_digits(
        &self,
        _value: FloatBits,
        _upper: bool,
        _ndigits: i32,
        _out: &mut [u8],
    ) -> Result<DigitsOut, FmtErr> {
        Err(FmtErr::Invalid)
    }
}

/// The `%m` text source.
pub(crate) fn errno_text(errnum: i32, buffer: &mut [u8; 64]) -> usize {
    let text = raven_libc_str::errmsg::strerror::message(errnum).unwrap_or(b"Unknown error");
    let copied = core::cmp::min(text.len(), buffer.len().saturating_sub(1));
    if let (Some(target), Some(source)) = (buffer.get_mut(..copied), text.get(..copied)) {
        target.copy_from_slice(source);
    }
    copied
}

/// The `%m`/`asprintf` allocator.
pub(crate) fn alloc(size: usize) -> *mut u8 {
    // SAFETY: the allocator returns `size` writable bytes or null.
    unsafe { crate::malloc::abi_alloc(size) }
}

/// The options of one call.
pub(crate) fn options() -> FmtOptions<'static> {
    FmtOptions {
        serrno: crate::ffi::errno::get(),
        ..FmtOptions::default()
    }
}

/// `vprintf`: format to `stdout`.
///
/// # Safety
/// `format` is NUL terminated and `list` is the trampoline's `va_list`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn vprintf(format: *const c_char, list: *mut List) -> c_int {
    // SAFETY: the caller's format contract.
    let format = unsafe { crate::ffi::cstr::slice(format) };
    // SAFETY: the trampoline built the list.
    let mut env = crate::va::CEnv::new(unsafe { reader(list) }, errno_text, alloc);
    let options = options();
    match raven_libc_stdio::vfprintf_file(
        crate::stdio::streams::streams().stdout(),
        format,
        &mut env,
        &options,
        &FloatHook,
    ) {
        Ok(value) => value,
        Err(error) => crate::ffi::map_fmt::apply(error),
    }
}
