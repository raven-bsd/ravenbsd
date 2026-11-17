// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `mbsrtowcs`, `mbsnrtowcs`, `wcsrtombs` and `wcsnrtombs`.

use core::ffi::{c_char, c_int};
use core::ptr;

use raven_libc_ctype::utf8::{SrcPos, StrConv};
use raven_libc_ctype::{MbMode, MbState};

const EILSEQ: c_int = 84;

fn result(value: StrConv) -> isize {
    match value.ret {
        Ok(count) => isize::try_from(count).unwrap_or(isize::MAX),
        Err(_) => {
            crate::ffi::errno::set_raw(EILSEQ);
            -1
        }
    }
}

fn byte_length(src: *const c_char) -> usize {
    let mut length = 0usize;
    // SAFETY: the caller's source is NUL-terminated.
    while unsafe { *src.add(length) } != 0 {
        length += 1;
    }
    length
}

fn wide_length(src: *const u32) -> usize {
    let mut length = 0usize;
    // SAFETY: the caller's source is NUL-terminated.
    while unsafe { *src.add(length) } != 0 {
        length += 1;
    }
    length
}

fn update_byte_source(position: SrcPos, original: *const c_char, target: *mut *const c_char) {
    if target.is_null() {
        return;
    }
    // SAFETY: the caller grants a writable source pointer.
    unsafe {
        *target = match position {
            SrcPos::Nul => ptr::null(),
            SrcPos::At(offset) => original.add(offset),
        };
    }
}

fn update_wide_source(position: SrcPos, original: *const u32, target: *mut *const u32) {
    if target.is_null() {
        return;
    }
    // SAFETY: the caller grants a writable source pointer.
    unsafe {
        *target = match position {
            SrcPos::Nul => ptr::null(),
            SrcPos::At(offset) => original.add(offset),
        };
    }
}

/// `mbsrtowcs`.
///
/// # Safety
/// `src` is a readable source pointer; `dst` has `len` writable units; `ps` is NULL or writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mbsrtowcs(
    dst: *mut u32,
    src: *mut *const c_char,
    len: usize,
    ps: *mut MbState,
) -> usize {
    if src.is_null() {
        return 0;
    }
    // SAFETY: the caller grants a readable source pointer.
    let original = unsafe { *src };
    if original.is_null() {
        return 0;
    }
    let length = byte_length(original);
    // SAFETY: the NUL is readable and part of the source.
    let input = unsafe { core::slice::from_raw_parts(original.cast::<u8>(), length + 1) };
    let output = if dst.is_null() {
        None
    } else {
        // SAFETY: the caller grants `len` writable wide units.
        Some(unsafe { core::slice::from_raw_parts_mut(dst, len) })
    };
    let mut state = unsafe { ps.as_ref() }.copied().unwrap_or(MbState::INITIAL);
    let converted =
        raven_libc_ctype::utf8::mbsrtowcs(Some(&mut state), MbMode::Utf8, input, output);
    if !ps.is_null() {
        // SAFETY: the caller grants a writable state slot.
        unsafe {
            *ps = converted.state;
        }
    }
    update_byte_source(converted.src, original, src);
    result(converted) as usize
}

/// `mbsnrtowcs`.
///
/// # Safety
/// As `mbsrtowcs`; `nms` bounds the source bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mbsnrtowcs(
    dst: *mut u32,
    src: *mut *const c_char,
    nms: usize,
    len: usize,
    ps: *mut MbState,
) -> usize {
    if src.is_null() {
        return 0;
    }
    // SAFETY: the caller grants a readable source pointer.
    let original = unsafe { *src };
    if original.is_null() {
        return 0;
    }
    // SAFETY: the caller grants `nms` readable bytes.
    let input = unsafe { core::slice::from_raw_parts(original.cast::<u8>(), nms) };
    let output = if dst.is_null() {
        None
    } else {
        // SAFETY: the caller grants `len` writable wide units.
        Some(unsafe { core::slice::from_raw_parts_mut(dst, len) })
    };
    let mut state = unsafe { ps.as_ref() }.copied().unwrap_or(MbState::INITIAL);
    let converted =
        raven_libc_ctype::utf8::mbsnrtowcs(Some(&mut state), MbMode::Utf8, input, output);
    if !ps.is_null() {
        // SAFETY: the caller grants a writable state slot.
        unsafe {
            *ps = converted.state;
        }
    }
    update_byte_source(converted.src, original, src);
    result(converted) as usize
}

/// `wcsrtombs`.
///
/// # Safety
/// `src` is a readable source pointer; `dst` has `len` writable bytes; `ps` is NULL or writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn wcsrtombs(
    dst: *mut c_char,
    src: *mut *const u32,
    len: usize,
    ps: *mut MbState,
) -> usize {
    if src.is_null() {
        return 0;
    }
    // SAFETY: the caller grants a readable source pointer.
    let original = unsafe { *src };
    if original.is_null() {
        return 0;
    }
    let length = wide_length(original);
    // SAFETY: the NUL is readable and part of the source.
    let input = unsafe { core::slice::from_raw_parts(original, length + 1) };
    let output = if dst.is_null() {
        None
    } else {
        // SAFETY: the caller grants `len` writable bytes.
        Some(unsafe { core::slice::from_raw_parts_mut(dst.cast::<u8>(), len) })
    };
    let mut state = unsafe { ps.as_ref() }.copied().unwrap_or(MbState::INITIAL);
    let converted =
        raven_libc_ctype::utf8::wcsrtombs(Some(&mut state), MbMode::Utf8, input, output);
    if !ps.is_null() {
        // SAFETY: the caller grants a writable state slot.
        unsafe {
            *ps = converted.state;
        }
    }
    update_wide_source(converted.src, original, src);
    result(converted) as usize
}

/// `wcsnrtombs`.
///
/// # Safety
/// As `wcsrtombs`; `nwc` bounds the source wide units.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn wcsnrtombs(
    dst: *mut c_char,
    src: *mut *const u32,
    nwc: usize,
    len: usize,
    ps: *mut MbState,
) -> usize {
    if src.is_null() {
        return 0;
    }
    // SAFETY: the caller grants a readable source pointer.
    let original = unsafe { *src };
    if original.is_null() {
        return 0;
    }
    // SAFETY: the caller grants `nwc` readable wide units.
    let input = unsafe { core::slice::from_raw_parts(original, nwc) };
    let output = if dst.is_null() {
        None
    } else {
        // SAFETY: the caller grants `len` writable bytes.
        Some(unsafe { core::slice::from_raw_parts_mut(dst.cast::<u8>(), len) })
    };
    let mut state = unsafe { ps.as_ref() }.copied().unwrap_or(MbState::INITIAL);
    let converted =
        raven_libc_ctype::utf8::wcsnrtombs(Some(&mut state), MbMode::Utf8, input, output);
    if !ps.is_null() {
        // SAFETY: the caller grants a writable state slot.
        unsafe {
            *ps = converted.state;
        }
    }
    update_wide_source(converted.src, original, src);
    result(converted) as usize
}
