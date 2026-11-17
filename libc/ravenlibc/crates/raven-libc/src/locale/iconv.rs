// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `iconv` from `<iconv.h>`.

use core::ffi::{c_char, c_void};

use raven_libc_abi::errno::Errno;
use raven_libc_iconv::Iconv;

use super::iconv_support::set_errno;

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
