// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! Unit tests for [`crate::cookie`] with Rust `extern "C"` callbacks.

#![allow(
    clippy::undocumented_unsafe_blocks,
    clippy::bool_assert_comparison,
    unsafe_code,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects
)]

use core::ffi::{c_char, c_int, c_void};

use super::*;
use crate::fake_ut::FakeEnv;
use crate::raw::CookieIo;

struct Cookie {
    data: Vec<u8>,
    pos: usize,
    closed: bool,
}

unsafe extern "C" fn cookie_read(cookie: *mut c_void, buf: *mut c_char, size: usize) -> isize {
    // SAFETY: the pointer comes from the Box allocated in this test (or the caller contract of the C API) and is live for the call.
    let cookie = unsafe { &mut *cookie.cast::<Cookie>() };
    let available = cookie.data.len().saturating_sub(cookie.pos);
    let count = available.min(size);
    if count != 0 {
        // SAFETY: the pointer comes from the Box allocated in this test (or the caller contract of the C API) and is live for the call.
        unsafe {
            core::ptr::copy_nonoverlapping(
                cookie.data.as_ptr().add(cookie.pos),
                buf.cast::<u8>(),
                count,
            );
        }
        cookie.pos += count;
    }
    count as isize
}

unsafe extern "C" fn cookie_write(cookie: *mut c_void, buf: *const c_char, size: usize) -> isize {
    // SAFETY: the pointer comes from the Box allocated in this test (or the caller contract of the C API) and is live for the call.
    let cookie = unsafe { &mut *cookie.cast::<Cookie>() };
    // SAFETY: the pointer comes from the Box allocated in this test (or the caller contract of the C API) and is live for the call.
    let bytes = unsafe { core::slice::from_raw_parts(buf.cast::<u8>(), size) };
    if cookie.pos > cookie.data.len() {
        cookie.data.resize(cookie.pos, 0);
    }
    if cookie.pos + size > cookie.data.len() {
        cookie.data.resize(cookie.pos + size, 0);
    }
    cookie.data[cookie.pos..cookie.pos + size].copy_from_slice(bytes);
    cookie.pos += size;
    size as isize
}

unsafe extern "C" fn cookie_seek(cookie: *mut c_void, offset: *mut i64, whence: c_int) -> c_int {
    // SAFETY: the pointer comes from the Box allocated in this test (or the caller contract of the C API) and is live for the call.
    let cookie = unsafe { &mut *cookie.cast::<Cookie>() };
    let base = match whence {
        0 => 0i64,
        1 => cookie.pos as i64,
        _ => cookie.data.len() as i64,
    };
    // SAFETY: the pointer comes from the Box allocated in this test (or the caller contract of the C API) and is live for the call.
    let Some(new) = base.checked_add(unsafe { *offset }) else {
        return -1;
    };
    if new < 0 {
        return -1;
    }
    cookie.pos = new as usize;
    // SAFETY: the pointer comes from the Box allocated in this test (or the caller contract of the C API) and is live for the call.
    unsafe { *offset = new };
    0
}

unsafe extern "C" fn cookie_close(cookie: *mut c_void) -> c_int {
    // SAFETY: the pointer comes from the Box allocated in this test (or the caller contract of the C API) and is live for the call.
    let cookie = unsafe { &mut *cookie.cast::<Cookie>() };
    cookie.closed = true;
    0
}

fn make_cookie() -> *mut Cookie {
    std::boxed::Box::into_raw(std::boxed::Box::new(Cookie {
        data: Vec::new(),
        pos: 0,
        closed: false,
    }))
}

#[test]
fn fopencookie_reads_writes_and_seeks() {
    let state: &'static crate::state::State<FakeEnv> = crate::fake_ut::leaked_state();
    let cookie = make_cookie();
    let io = CookieIo {
        read: Some(cookie_read),
        write: Some(cookie_write),
        seek: Some(cookie_seek),
        close: Some(cookie_close),
    };
    let pointer = fopencookie(state, cookie.cast(), b"w+", io).expect("fopencookie");
    let file = crate::raw::file_ref(pointer);
    assert_eq!(crate::block_io::fwrite(file, b"hello", 1, 5), Ok(5));
    crate::flush::fflush(state, Some(file)).expect("fflush");
    assert_eq!(crate::seek::ftell(file), Ok(5));
    assert_eq!(crate::seek::fseek(file, 0, 0), Ok(()));
    assert_eq!(crate::char_io::getc(file), Ok(i32::from(b'h')));
    assert_eq!(crate::char_io::getc(file), Ok(i32::from(b'e')));
    assert_eq!(crate::close::fclose(file), Ok(()));
    let raw = cookie;
    // SAFETY: the pointer comes from the Box allocated in this test (or the caller contract of the C API) and is live for the call.
    let cookie = unsafe { &*raw };
    assert_eq!(cookie.data, b"hello".to_vec());
    assert!(cookie.closed);
    // SAFETY: the pointer comes from the Box allocated in this test (or the caller contract of the C API) and is live for the call.
    unsafe { drop(std::boxed::Box::from_raw(raw)) };
}

#[test]
fn fopencookie_without_a_seek_callback_is_espipe() {
    let state: &'static crate::state::State<FakeEnv> = crate::fake_ut::leaked_state();
    let cookie = make_cookie();
    let io = CookieIo {
        read: Some(cookie_read),
        write: Some(cookie_write),
        seek: None,
        close: Some(cookie_close),
    };
    let pointer = fopencookie(state, cookie.cast(), b"r+", io).expect("fopencookie");
    let file = crate::raw::file_ref(pointer);
    assert_eq!(
        crate::seek::fseek(file, 0, 0),
        Err(Fail::from_int(raven_libc_abi::generated::errno::ENOTSUP))
    );
    // SAFETY: the pointer comes from the Box allocated in this test (or the caller contract of the C API) and is live for the call.
    unsafe { drop(std::boxed::Box::from_raw(cookie)) };
}

#[test]
fn fopencookie_without_a_write_callback_returns_the_length() {
    let state: &'static crate::state::State<FakeEnv> = crate::fake_ut::leaked_state();
    let cookie = make_cookie();
    let io = CookieIo {
        read: Some(cookie_read),
        write: None,
        seek: Some(cookie_seek),
        close: Some(cookie_close),
    };
    let pointer = fopencookie(state, cookie.cast(), b"w", io).expect("fopencookie");
    let file = crate::raw::file_ref(pointer);
    assert_eq!(crate::block_io::fwrite(file, b"abc", 1, 3), Ok(3));
    assert_eq!(crate::flush::fflush(state, Some(file)), Ok(()));
    // SAFETY: the pointer comes from the Box allocated in this test (or the caller contract of the C API) and is live for the call.
    unsafe { drop(std::boxed::Box::from_raw(cookie)) };
}

#[test]
fn fopencookie_rejects_a_bad_mode() {
    let state: &'static crate::state::State<FakeEnv> = crate::fake_ut::leaked_state();
    let cookie = make_cookie();
    assert_eq!(
        fopencookie(state, cookie.cast(), b"q", CookieIo::default()).map(|_| ()),
        Err(Fail::from_int(raven_libc_abi::generated::errno::EINVAL))
    );
    // SAFETY: the pointer comes from the Box allocated in this test (or the caller contract of the C API) and is live for the call.
    unsafe { drop(std::boxed::Box::from_raw(cookie)) };
}
