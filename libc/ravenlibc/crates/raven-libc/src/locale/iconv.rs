// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `<iconv.h>` exports over C48's `raven-libc-iconv`.

use alloc::boxed::Box;
use core::ffi::{c_char, c_void};

use raven_libc_abi::errno::Errno;
use raven_libc_iconv::{Iconv, IconvError};

/// Map a conversion failure to the C errno.
fn set_errno(error: IconvError) {
    let value = match error {
        IconvError::TooBig => 7,
        IconvError::Unsupported | IconvError::Incomplete => 22,
        IconvError::IllegalSequence => 84,
    };
    crate::ffi::errno::set(Errno(value));
}

/// `iconv_open`.
///
/// # Safety
/// Both strings are NUL terminated.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn iconv_open(to: *const c_char, from: *const c_char) -> *mut c_void {
    if to.is_null() || from.is_null() {
        return crate::ffi::ret::null(Errno(22));
    }
    // SAFETY: the caller's C string contract.
    let (to, from) = unsafe { (crate::ffi::cstr::slice(to), crate::ffi::cstr::slice(from)) };
    match raven_libc_iconv::iconv_open(to, from) {
        Ok(conversion) => Box::into_raw(Box::new(conversion)).cast::<c_void>(),
        Err(_) => crate::ffi::ret::null(Errno(22)),
    }
}

/// `iconv`.
///
/// # Safety
/// `cd` is an open descriptor or `(iconv_t)-1`; the pointer arguments follow the C contract.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn iconv(
    cd: *mut c_void,
    inbuf: *mut *mut c_char,
    inleft: *mut usize,
    outbuf: *mut *mut c_char,
    outleft: *mut usize,
) -> usize {
    if cd.is_null() || cd as usize == usize::MAX {
        crate::ffi::errno::set(Errno(9));
        return usize::MAX;
    }
    // SAFETY: `cd` came from `iconv_open`.
    let conversion = unsafe { &mut *(cd.cast::<Iconv>()) };
    if inbuf.is_null() || outbuf.is_null() {
        conversion.reset();
        return 0;
    }
    // SAFETY: the caller's in/out buffer contracts.
    let (input_ptr, input_len, output_ptr, output_len) = unsafe {
        (
            *inbuf,
            if inleft.is_null() { 0 } else { *inleft },
            *outbuf,
            if outleft.is_null() { 0 } else { *outleft },
        )
    };
    let mut input: &[u8] = if input_ptr.is_null() || input_len == 0 {
        &[]
    } else {
        // SAFETY: the caller grants `input_len` readable bytes.
        unsafe { core::slice::from_raw_parts(input_ptr.cast::<u8>(), input_len) }
    };
    let mut output: &mut [u8] = if output_ptr.is_null() || output_len == 0 {
        &mut []
    } else {
        // SAFETY: the caller grants `output_len` writable bytes.
        unsafe { core::slice::from_raw_parts_mut(output_ptr.cast::<u8>(), output_len) }
    };
    let result = conversion.convert(&mut input, &mut output);
    let consumed = input_len.saturating_sub(input.len());
    let produced = output_len.saturating_sub(output.len());
    // SAFETY: the pointers stay inside the caller's buffers; the remainder is reported back.
    unsafe {
        *inbuf = input_ptr.add(consumed);
        if !inleft.is_null() {
            *inleft = input.len();
        }
        *outbuf = output_ptr.add(produced);
        if !outleft.is_null() {
            *outleft = output.len();
        }
    }
    match result {
        Ok(irreversible) => irreversible,
        Err(error) => {
            set_errno(error);
            usize::MAX
        }
    }
}

/// `iconv_close`.
///
/// # Safety
/// `cd` is a descriptor from `iconv_open` or `(iconv_t)-1`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn iconv_close(cd: *mut c_void) -> core::ffi::c_int {
    if cd.is_null() || cd as usize == usize::MAX {
        crate::ffi::errno::set(Errno(9));
        return -1;
    }
    // SAFETY: `cd` is a Box from `iconv_open`.
    drop(unsafe { Box::from_raw(cd.cast::<Iconv>()) });
    0
}
