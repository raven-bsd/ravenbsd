// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//!
//! The `v*` targets of the wide `printf`/`scanf` trampolines.

use core::ffi::{c_char, c_int, c_void};
use core::sync::atomic::Ordering;

use raven_libc_fmt::glue::vswprintf_core;
use raven_libc_fmt::types::FmtErr;

use crate::stdio::va::{List, reader};

fn wide_format<'a>(format: *const u32) -> &'a [u32] {
    if format.is_null() {
        return &[];
    }
    let mut length = 0usize;
    // SAFETY: the caller's format has a NUL; the walk cannot pass it.
    while unsafe { *format.add(length) } != 0 {
        length = length.wrapping_add(1);
    }
    // SAFETY: `length` units precede the NUL the walk found.
    unsafe { core::slice::from_raw_parts(format, length) }
}

/// `vwprintf`: format to stdout.
///
/// # Safety
/// `format` is NUL-terminated and `list` is the trampoline list.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn vwprintf(format: *const u32, list: *mut List) -> c_int {
    let stream = crate::stdio::stdout::stdout.load(Ordering::Acquire);
    if stream.is_null() {
        return crate::ffi::ret::int(Err(raven_libc_abi::errno::Errno(9)));
    }
    // SAFETY: forwarded C contract.
    unsafe { super::vfwprintf::vfwprintf(stream, format, list) }
}

/// `vswprintf`: format into a bounded wide buffer.
///
/// # Safety
/// `out` has `cap` writable units; `format` is NUL-terminated; `list` is the trampoline list.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn vswprintf(
    out: *mut u32,
    cap: usize,
    format: *const u32,
    list: *mut List,
) -> c_int {
    if out.is_null() || cap == 0 {
        return crate::ffi::ret::int(Err(raven_libc_abi::errno::Errno(22)));
    }
    // SAFETY: the caller grants `cap` writable wide units.
    let buffer = unsafe { core::slice::from_raw_parts_mut(out, cap) };
    let format = wide_format(format);
    // SAFETY: the trampoline built the list.
    let mut env = crate::va::CEnv::new(
        unsafe { reader(list) },
        crate::stdio::vprintf::errno_text,
        crate::stdio::vprintf::alloc,
    );
    let options = crate::stdio::vprintf::options();
    match vswprintf_core(buffer, cap, format, &mut env, &options, &crate::stdio::vprintf::FloatHook) {
        Ok(value) => value,
        Err(FmtErr::Invalid) => crate::ffi::ret::int(Err(raven_libc_abi::errno::Errno(22))),
        Err(_) => -1,
    }
}

fn scan_result(
    result: Result<raven_libc_fmt::ScanResult, FmtErr>,
) -> c_int {
    match result {
        Ok(value) if value.return_eof => -1,
        Ok(value) => value.assigned,
        Err(error) => crate::ffi::map_fmt::apply(error),
    }
}

/// `vfwscanf`: scan from a FILE with a wide format.
///
/// # Safety
/// `stream` is a FILE, `format` is NUL-terminated and `list` is the trampoline list.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn vfwscanf(
    stream: *mut c_void,
    format: *const u32,
    list: *mut List,
) -> c_int {
    if stream.is_null() || format.is_null() {
        return crate::ffi::ret::int(Err(raven_libc_abi::errno::Errno(22)));
    }
    let target: &'static raven_libc_stdio::File<crate::ffi::env::ShellEnv> =
        raven_libc_stdio::file_from_ptr(stream.cast());
    let format = wide_format(format);
    // SAFETY: the trampoline built the list.
    let mut env = crate::va::CEnv::new(
        unsafe { reader(list) },
        crate::stdio::vprintf::errno_text,
        crate::stdio::vprintf::alloc,
    );
    let options = crate::stdio::vsscanf::scan_options();
    scan_result(raven_libc_stdio::vfwscanf_file(
        target,
        format,
        &mut env,
        &options,
        &crate::stdio::vsscanf::ScanFloat,
    ))
}

/// `vwscanf`: scan stdin with a wide format.
///
/// # Safety
/// `format` is NUL-terminated and `list` is the trampoline list.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn vwscanf(format: *const u32, list: *mut List) -> c_int {
    let stream = crate::stdio::stdin::stdin.load(Ordering::Acquire);
    if stream.is_null() {
        return crate::ffi::ret::int(Err(raven_libc_abi::errno::Errno(9)));
    }
    // SAFETY: forwarded C contract.
    unsafe { vfwscanf(stream, format, list) }
}
