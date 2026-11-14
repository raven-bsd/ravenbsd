// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//!
//! The `scanf` family: the C56 file glue and a shell byte source for `sscanf`.

use core::ffi::{c_char, c_int, c_void};

use raven_libc_abi::errno::Errno;
use raven_libc_fmt::{
    FloatBits, FloatKind, FloatScan, FmtErr, ScanOptions, ScanResult, ScanSource, vscan,
};
use raven_libc_stdio::Fail;
use raven_libc_sys::weak_hook;

use crate::stdio::streams::streams;
use crate::stdio::va::{List, reader};

/// A byte source over a C string for `sscanf`.
struct SliceSource<'a> {
    data: &'a [u8],
    position: usize,
}

impl<'a> SliceSource<'a> {
    fn new(data: &'a [u8]) -> Self {
        Self { data, position: 0 }
    }
}

impl ScanSource<u8> for SliceSource<'_> {
    fn getc(&mut self) -> Option<u8> {
        let value = self.data.get(self.position).copied();
        if value.is_some() {
            self.position = self.position.saturating_add(1);
        }
        value
    }

    fn ungetc(&mut self, _c: u8) {
        self.position = self.position.saturating_sub(1);
    }
}

/// The `scanf` float dummy: fail the match.
fn dummy_scan_float(_token: &[u8], _single: bool, _bits: &mut u64) -> bool {
    false
}

weak_hook! {
    /// The float-scan plug-in of `hooks.toml`; C61's start path installs the strong member.
    pub(crate) fn __rl_scan_float(token: &[u8], single: bool, bits: &mut u64) -> bool;
    dummy = crate::stdio::vsscanf::dummy_scan_float;
}

struct ScanFloat;

impl FloatScan for ScanFloat {
    fn parse(&self, token: &[u8], kind: FloatKind) -> Result<FloatBits, FmtErr> {
        let mut bits = 0u64;
        let parsed = match kind {
            FloatKind::F32 => __rl_scan_float(token, true, &mut bits),
            FloatKind::F64 => __rl_scan_float(token, false, &mut bits),
            FloatKind::LongDouble => false,
        };
        if parsed {
            Ok(FloatBits {
                kind,
                bits: u128::from(bits),
            })
        } else {
            Err(FmtErr::Invalid)
        }
    }
}

fn result(result: Result<ScanResult, FmtErr>) -> c_int {
    match result {
        Ok(value) => {
            if value.return_eof {
                -1
            } else {
                value.assigned
            }
        }
        Err(_) => {
            crate::ffi::errno::set(Errno(22));
            -1
        }
    }
}

fn scan_options() -> ScanOptions<'static> {
    ScanOptions::default()
}

/// `vsscanf`.
///
/// # Safety
/// `input` and `format` are NUL terminated and `list` is the trampoline's list.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn vsscanf(
    input: *const c_char,
    format: *const c_char,
    list: *mut List,
) -> c_int {
    if input.is_null() || format.is_null() {
        return crate::ffi::ret::int(Err(Errno(22)));
    }
    // SAFETY: both strings are NUL terminated.
    let input = unsafe { crate::ffi::cstr::slice(input) };
    let format = unsafe { crate::ffi::cstr::slice(format) };
    let mut source = SliceSource::new(input);
    let mut env = crate::va::CEnv::new(
        unsafe { reader(list) },
        crate::stdio::vprintf::errno_text,
        crate::stdio::vprintf::alloc,
    );
    let options = scan_options();
    result(vscan(&mut source, format, &mut env, &options, &ScanFloat))
}

/// `vscanf`.
///
/// # Safety
/// `format` is NUL terminated and `list` is the trampoline's list.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn vscanf(format: *const c_char, list: *mut List) -> c_int {
    if format.is_null() {
        return crate::ffi::ret::int(Err(Errno(22)));
    }
    // SAFETY: the caller's format contract.
    let format = unsafe { crate::ffi::cstr::slice(format) };
    let target = streams().stdin();
    // SAFETY: the trampoline built the list.
    let mut env = crate::va::CEnv::new(
        unsafe { reader(list) },
        crate::stdio::vprintf::errno_text,
        crate::stdio::vprintf::alloc,
    );
    let options = scan_options();
    result(raven_libc_stdio::vfscanf_file(
        target, format, &mut env, &options, &ScanFloat,
    ))
}

/// `vfscanf`.
///
/// # Safety
/// `stream` is a FILE, `format` is NUL terminated and `list` is the trampoline's list.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn vfscanf(
    stream: *mut c_void,
    format: *const c_char,
    list: *mut List,
) -> c_int {
    if stream.is_null() || format.is_null() {
        return crate::ffi::ret::int(Err(Errno(22)));
    }
    let target: &'static raven_libc_stdio::File<crate::ffi::env::ShellEnv> =
        raven_libc_stdio::file_from_ptr(stream.cast());
    // SAFETY: the caller's format and list contracts.
    let format = unsafe { crate::ffi::cstr::slice(format) };
    let mut env = crate::va::CEnv::new(
        unsafe { reader(list) },
        crate::stdio::vprintf::errno_text,
        crate::stdio::vprintf::alloc,
    );
    let options = scan_options();
    result(raven_libc_stdio::vfscanf_file(
        target, format, &mut env, &options, &ScanFloat,
    ))
}

/// `fscanf` failure class used by the crate.
#[allow(dead_code)]
fn fail(error: Fail) -> c_int {
    if let Fail::Errno(value) = error {
        crate::ffi::errno::set(value);
    }
    -1
}
