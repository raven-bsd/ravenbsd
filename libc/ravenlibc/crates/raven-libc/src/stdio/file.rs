// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//!
//! The `FILE` half of `<stdio.h>`: open/close, block and line I/O, seek, buffering and memory
//! streams. Every body forwards to a C56 crate function and maps `Fail` to the C return/errno.

use core::ffi::{c_char, c_int, c_long, c_void};

use raven_libc_abi::errno::Errno;
use raven_libc_stdio::{EOF, Fail, File, R};

use crate::ffi::errno;
use crate::stdio::streams::streams;

type ShellFile = File<crate::ffi::env::ShellEnv>;

fn file(pointer: *mut c_void) -> Option<&'static ShellFile> {
    if pointer.is_null() {
        None
    } else {
        Some(raven_libc_stdio::file_from_ptr(pointer.cast()))
    }
}

fn fail_int(error: Fail) -> c_int {
    if let Fail::Errno(value) = error {
        errno::set(value);
    }
    EOF
}

fn fail_ptr<T>(error: Fail) -> *mut T {
    if let Fail::Errno(value) = error {
        errno::set(value);
    }
    core::ptr::null_mut()
}

fn fail_zero(error: Fail) -> usize {
    if let Fail::Errno(value) = error {
        errno::set(value);
    }
    0
}

/// `fopen`.
///
/// # Safety
/// `path` and `mode` are NUL-terminated strings.
pub(crate) unsafe extern "C" fn fopen_impl(
    path: *const c_char,
    mode: *const c_char,
) -> *mut ShellFile {
    if path.is_null() || mode.is_null() {
        return crate::ffi::ret::null(Errno(22));
    }
    // SAFETY: both arguments are NUL-terminated by the C contract.
    let path = unsafe { crate::ffi::cstr::slice(path) };
    let mode = unsafe { crate::ffi::cstr::slice(mode) };
    match raven_libc_stdio::fopen(streams().state(), path, mode) {
        Ok(pointer) => pointer,
        Err(error) => fail_ptr(error),
    }
}

/// `fdopen`.
///
/// # Safety
/// `mode` is NUL terminated.
pub(crate) unsafe extern "C" fn fdopen_impl(fd: c_int, mode: *const c_char) -> *mut ShellFile {
    if mode.is_null() {
        return crate::ffi::ret::null(Errno(22));
    }
    // SAFETY: the mode is NUL terminated by the C contract.
    let mode = unsafe { crate::ffi::cstr::slice(mode) };
    match raven_libc_stdio::fdopen(streams().state(), fd, mode) {
        Ok(pointer) => pointer,
        Err(error) => fail_ptr(error),
    }
}

/// `freopen`.
///
/// # Safety
/// `path` is NULL or NUL terminated, `mode` is NUL terminated and `stream` is a FILE.
pub(crate) unsafe extern "C" fn freopen_impl(
    path: *const c_char,
    mode: *const c_char,
    stream: *mut c_void,
) -> *mut ShellFile {
    let Some(target) = file(stream) else {
        return crate::ffi::ret::null(Errno(22));
    };
    if mode.is_null() {
        return crate::ffi::ret::null(Errno(22));
    }
    // SAFETY: the C contract gives NUL-terminated strings.
    let name = if path.is_null() {
        None
    } else {
        Some(unsafe { crate::ffi::cstr::slice(path) })
    };
    let mode = unsafe { crate::ffi::cstr::slice(mode) };
    match raven_libc_stdio::freopen(streams().state(), name, mode, target) {
        Ok(pointer) => pointer,
        Err(error) => fail_ptr(error),
    }
}

/// `fclose`.
///
/// # Safety
/// `stream` is a FILE.
pub(crate) unsafe extern "C" fn fclose_impl(stream: *mut c_void) -> c_int {
    let Some(target) = file(stream) else {
        return crate::ffi::ret::int(Err(Errno(9)));
    };
    match raven_libc_stdio::fclose(target) {
        Ok(()) => 0,
        Err(error) => fail_int(error),
    }
}

/// `fflush`.
///
/// # Safety
/// `stream` is NULL or a FILE.
pub(crate) unsafe extern "C" fn fflush_impl(stream: *mut c_void) -> c_int {
    let target = if stream.is_null() { None } else { file(stream) };
    if !stream.is_null() && target.is_none() {
        return crate::ffi::ret::int(Err(Errno(9)));
    }
    match raven_libc_stdio::fflush(streams().state(), target) {
        Ok(()) => 0,
        Err(error) => fail_int(error),
    }
}

/// `fread`.
///
/// # Safety
/// `ptr` is writable for `size * nmemb` bytes and `stream` is a FILE.
pub(crate) unsafe extern "C" fn fread_impl(
    ptr: *mut c_void,
    size: usize,
    nmemb: usize,
    stream: *mut c_void,
) -> usize {
    let Some(target) = file(stream) else {
        return crate::ffi::ret::size(Err(Errno(9)));
    };
    let Some(length) = size.checked_mul(nmemb) else {
        return crate::ffi::ret::size(Err(Errno(75)));
    };
    if length == 0 {
        return 0;
    }
    // SAFETY: the caller granted `length` writable bytes.
    let buffer = unsafe { core::slice::from_raw_parts_mut(ptr.cast::<u8>(), length) };
    match raven_libc_stdio::fread(target, buffer, size, nmemb) {
        Ok(value) => value,
        Err(error) => fail_zero(error),
    }
}

/// `fwrite`.
///
/// # Safety
/// `ptr` is readable for `size * nmemb` bytes and `stream` is a FILE.
pub(crate) unsafe extern "C" fn fwrite_impl(
    ptr: *const c_void,
    size: usize,
    nmemb: usize,
    stream: *mut c_void,
) -> usize {
    let Some(target) = file(stream) else {
        return crate::ffi::ret::size(Err(Errno(9)));
    };
    let Some(length) = size.checked_mul(nmemb) else {
        return crate::ffi::ret::size(Err(Errno(75)));
    };
    if length == 0 {
        return 0;
    }
    // SAFETY: the caller granted `length` readable bytes.
    let buffer = unsafe { core::slice::from_raw_parts(ptr.cast::<u8>(), length) };
    match raven_libc_stdio::fwrite(target, buffer, size, nmemb) {
        Ok(value) => value,
        Err(error) => fail_zero(error),
    }
}

/// `fseek`.
///
/// # Safety
/// `stream` is a FILE.
pub(crate) unsafe extern "C" fn fseek_impl(
    stream: *mut c_void,
    offset: c_long,
    whence: c_int,
) -> c_int {
    let Some(target) = file(stream) else {
        return crate::ffi::ret::int(Err(Errno(9)));
    };
    match raven_libc_stdio::fseek(target, offset, whence) {
        Ok(()) => 0,
        Err(error) => fail_int(error),
    }
}

/// `ftell`.
///
/// # Safety
/// `stream` is a FILE.
pub(crate) unsafe extern "C" fn ftell_impl(stream: *mut c_void) -> c_long {
    let Some(target) = file(stream) else {
        return crate::ffi::ret::long(Err(Errno(9)));
    };
    match raven_libc_stdio::ftell(target) {
        Ok(value) => value,
        Err(error) => {
            if let Fail::Errno(value) = error {
                errno::set(value);
            }
            -1
        }
    }
}

/// `setvbuf`.
///
/// # Safety
/// `stream` is a FILE and `buf` is NULL or writable for `size` bytes.
pub(crate) unsafe extern "C" fn setvbuf_impl(
    stream: *mut c_void,
    buf: *mut c_char,
    mode: c_int,
    size: usize,
) -> c_int {
    let Some(target) = file(stream) else {
        return crate::ffi::ret::int(Err(Errno(9)));
    };
    let buffer = if buf.is_null() {
        None
    } else {
        Some((buf.cast::<u8>(), size))
    };
    match raven_libc_stdio::setvbuf(target, buffer, mode, size) {
        Ok(()) => 0,
        Err(error) => fail_int(error),
    }
}

/// `ungetc`.
///
/// # Safety
/// `stream` is a FILE.
pub(crate) unsafe extern "C" fn ungetc_impl(c: c_int, stream: *mut c_void) -> c_int {
    let Some(target) = file(stream) else {
        return crate::ffi::ret::int(Err(Errno(9)));
    };
    match raven_libc_stdio::ungetc(target, c) {
        Ok(value) => value,
        Err(error) => fail_int(error),
    }
}

/// `fgetc`.
///
/// # Safety
/// `stream` is a FILE.
pub(crate) unsafe extern "C" fn fgetc_impl(stream: *mut c_void) -> c_int {
    let Some(target) = file(stream) else {
        return crate::ffi::ret::int(Err(Errno(9)));
    };
    match raven_libc_stdio::fgetc(target) {
        Ok(value) => value,
        Err(error) => fail_int(error),
    }
}

/// `fputc`.
///
/// # Safety
/// `stream` is a FILE.
pub(crate) unsafe extern "C" fn fputc_impl(c: c_int, stream: *mut c_void) -> c_int {
    let Some(target) = file(stream) else {
        return crate::ffi::ret::int(Err(Errno(9)));
    };
    match raven_libc_stdio::fputc(target, c) {
        Ok(value) => value,
        Err(error) => fail_int(error),
    }
}

/// `getchar`.
pub(crate) extern "C" fn getchar_impl() -> c_int {
    match raven_libc_stdio::getchar(streams().state()) {
        Ok(value) => value,
        Err(error) => fail_int(error),
    }
}

/// `putchar`.
pub(crate) extern "C" fn putchar_impl(c: c_int) -> c_int {
    match raven_libc_stdio::putchar(streams().state(), c) {
        Ok(value) => value,
        Err(error) => fail_int(error),
    }
}

/// `fgets`.
///
/// # Safety
/// `dst` is writable for `size` bytes and `stream` is a FILE.
pub(crate) unsafe extern "C" fn fgets_impl(
    dst: *mut c_char,
    size: c_int,
    stream: *mut c_void,
) -> *mut c_char {
    let Some(target) = file(stream) else {
        return crate::ffi::ret::null(Errno(9));
    };
    if size <= 0 || dst.is_null() {
        return crate::ffi::ret::null(Errno(22));
    }
    let length = usize::try_from(size).unwrap_or(0);
    // SAFETY: the caller granted `length` writable bytes.
    let buffer = unsafe { core::slice::from_raw_parts_mut(dst.cast::<u8>(), length) };
    match raven_libc_stdio::fgets(target, buffer) {
        Ok(_) => dst,
        Err(error) => fail_ptr(error),
    }
}

/// `fputs`.
///
/// # Safety
/// `text` is NUL terminated and `stream` is a FILE.
pub(crate) unsafe extern "C" fn fputs_impl(text: *const c_char, stream: *mut c_void) -> c_int {
    let Some(target) = file(stream) else {
        return crate::ffi::ret::int(Err(Errno(9)));
    };
    if text.is_null() {
        return crate::ffi::ret::int(Err(Errno(22)));
    }
    // SAFETY: the string is NUL terminated.
    let text = unsafe { crate::ffi::cstr::slice(text) };
    match raven_libc_stdio::fputs(target, text) {
        Ok(()) => 0,
        Err(error) => fail_int(error),
    }
}

/// `puts`.
///
/// # Safety
/// `text` is NUL terminated.
pub(crate) unsafe extern "C" fn puts_impl(text: *const c_char) -> c_int {
    if text.is_null() {
        return crate::ffi::ret::int(Err(Errno(22)));
    }
    // SAFETY: the string is NUL terminated.
    let text = unsafe { crate::ffi::cstr::slice(text) };
    match raven_libc_stdio::puts(streams().state(), text) {
        Ok(()) => 0,
        Err(error) => fail_int(error),
    }
}

/// `feof`.
///
/// # Safety
/// `stream` is a FILE.
pub(crate) unsafe extern "C" fn feof_impl(stream: *mut c_void) -> c_int {
    file(stream).map_or(0, |target| c_int::from(raven_libc_stdio::feof(target)))
}

/// `ferror`.
///
/// # Safety
/// `stream` is a FILE.
pub(crate) unsafe extern "C" fn ferror_impl(stream: *mut c_void) -> c_int {
    file(stream).map_or(0, |target| c_int::from(raven_libc_stdio::ferror(target)))
}

/// `clearerr`.
///
/// # Safety
/// `stream` is a FILE.
pub(crate) unsafe extern "C" fn clearerr_impl(stream: *mut c_void) {
    if let Some(target) = file(stream) {
        raven_libc_stdio::clearerr(target);
    }
}

/// `fileno`.
///
/// # Safety
/// `stream` is a FILE.
pub(crate) unsafe extern "C" fn fileno_impl(stream: *mut c_void) -> c_int {
    let Some(target) = file(stream) else {
        return crate::ffi::ret::int(Err(Errno(9)));
    };
    match raven_libc_stdio::fileno(target) {
        Ok(value) => value,
        Err(error) => fail_int(error),
    }
}

/// `perror`.
///
/// # Safety
/// `message` is NULL or NUL terminated.
pub(crate) unsafe extern "C" fn perror_impl(message: *const c_char) {
    let message = if message.is_null() {
        &b""[..]
    } else {
        // SAFETY: the string is NUL terminated.
        unsafe { crate::ffi::cstr::slice(message) }
    };
    let code = errno::get();
    let mut text = [0u8; 64];
    let length = crate::stdio::vprintf::errno_text(code, &mut text);
    let text = text.get(..length).unwrap_or(&[]);
    let _ = raven_libc_stdio::perror(streams().state(), message, text);
}

/// `fmemopen`.
///
/// # Safety
/// `buffer` is NULL or writable for `size` bytes; `mode` is NUL terminated.
pub(crate) unsafe extern "C" fn fmemopen_impl(
    buffer: *mut c_void,
    size: usize,
    mode: *const c_char,
) -> *mut ShellFile {
    if mode.is_null() {
        return crate::ffi::ret::null(Errno(22));
    }
    // SAFETY: the mode is NUL terminated.
    let mode = unsafe { crate::ffi::cstr::slice(mode) };
    let buffer = if buffer.is_null() {
        None
    } else {
        Some((buffer.cast::<u8>(), size))
    };
    match raven_libc_stdio::fmemopen(streams().state(), buffer, size, mode) {
        Ok(pointer) => pointer,
        Err(error) => fail_ptr(error),
    }
}

/// `open_memstream`.
///
/// # Safety
/// `buffer` and `size` are non-NULL writable pointers.
pub(crate) unsafe extern "C" fn open_memstream_impl(
    buffer: *mut *mut c_char,
    size: *mut usize,
) -> *mut ShellFile {
    match raven_libc_stdio::open_memstream(streams().state(), buffer.cast::<*mut u8>(), size) {
        Ok(pointer) => pointer,
        Err(error) => fail_ptr(error),
    }
}

/// Convert an `R<()>` to the C `int`.
#[allow(dead_code)]
fn unit(result: R<()>) -> c_int {
    match result {
        Ok(()) => 0,
        Err(error) => fail_int(error),
    }
}

mod f01;
mod f02;
mod f03;
mod f04;
mod f05;
mod f06;
mod f07;
