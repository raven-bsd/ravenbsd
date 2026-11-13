// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! Unit tests for [`crate::funopen`]: the callback contract, `ESPIPE` without `seekfn` and
//! `EINVAL` when both directions are missing.

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

struct Fun {
    data: Vec<u8>,
    pos: usize,
    closed: bool,
}

unsafe extern "C" fn fun_read(cookie: *mut c_void, buf: *mut c_char, n: c_int) -> c_int {
    // SAFETY: the pointer comes from the Box allocated in this test (or the caller contract of the C API) and is live for the call.
    let fun = unsafe { &mut *cookie.cast::<Fun>() };
    let want = usize::try_from(n).unwrap_or(0);
    let available = fun.data.len().saturating_sub(fun.pos);
    let count = available.min(want);
    if count != 0 {
        // SAFETY: the pointer comes from the Box allocated in this test (or the caller contract of the C API) and is live for the call.
        unsafe {
            core::ptr::copy_nonoverlapping(fun.data.as_ptr().add(fun.pos), buf.cast::<u8>(), count);
        }
        fun.pos += count;
    }
    count as c_int
}

unsafe extern "C" fn fun_write(cookie: *mut c_void, buf: *const c_char, n: c_int) -> c_int {
    // SAFETY: the pointer comes from the Box allocated in this test (or the caller contract of the C API) and is live for the call.
    let fun = unsafe { &mut *cookie.cast::<Fun>() };
    let want = usize::try_from(n).unwrap_or(0);
    // SAFETY: the pointer comes from the Box allocated in this test (or the caller contract of the C API) and is live for the call.
    let bytes = unsafe { core::slice::from_raw_parts(buf.cast::<u8>(), want) };
    fun.data.extend_from_slice(bytes);
    fun.pos = fun.data.len();
    n
}

unsafe extern "C" fn fun_seek(cookie: *mut c_void, off: i64, whence: c_int) -> i64 {
    // SAFETY: the pointer comes from the Box allocated in this test (or the caller contract of the C API) and is live for the call.
    let fun = unsafe { &mut *cookie.cast::<Fun>() };
    let base = match whence {
        0 => 0i64,
        1 => fun.pos as i64,
        _ => fun.data.len() as i64,
    };
    let new = base.saturating_add(off);
    if new < 0 {
        return -1;
    }
    fun.pos = new as usize;
    new
}

unsafe extern "C" fn fun_close(cookie: *mut c_void) -> c_int {
    // SAFETY: the pointer comes from the Box allocated in this test (or the caller contract of the C API) and is live for the call.
    let fun = unsafe { &mut *cookie.cast::<Fun>() };
    fun.closed = true;
    0
}

fn make() -> *mut Fun {
    std::boxed::Box::into_raw(std::boxed::Box::new(Fun {
        data: Vec::new(),
        pos: 0,
        closed: false,
    }))
}

#[test]
fn funopen_reads_writes_and_seeks() {
    let state: &'static crate::state::State<FakeEnv> = crate::fake_ut::leaked_state();
    let cookie = make();
    let pointer = funopen(
        state,
        cookie.cast(),
        Some(fun_read),
        Some(fun_write),
        Some(fun_seek),
        Some(fun_close),
    )
    .expect("funopen");
    let file = crate::raw::file_ref(pointer);
    assert_eq!(crate::block_io::fwrite(file, b"abc", 1, 3), Ok(3));
    crate::flush::fflush(state, Some(file)).expect("fflush");
    assert_eq!(crate::seek::ftell(file), Ok(3));
    assert_eq!(crate::seek::fseek(file, 0, 0), Ok(()));
    assert_eq!(crate::char_io::getc(file), Ok(i32::from(b'a')));
    assert_eq!(crate::close::fclose(file), Ok(()));
    let raw = cookie;
    // SAFETY: the pointer comes from the Box allocated in this test (or the caller contract of the C API) and is live for the call.
    let fun = unsafe { &*raw };
    assert_eq!(fun.data, b"abc".to_vec());
    assert!(fun.closed);
    // SAFETY: the pointer comes from the Box allocated in this test (or the caller contract of the C API) and is live for the call.
    unsafe { drop(std::boxed::Box::from_raw(raw)) };
}

#[test]
fn funopen_without_a_seek_function_is_espipe() {
    let state: &'static crate::state::State<FakeEnv> = crate::fake_ut::leaked_state();
    let cookie = make();
    let pointer = funopen(
        state,
        cookie.cast(),
        Some(fun_read),
        None,
        None,
        Some(fun_close),
    )
    .expect("funopen");
    let file = crate::raw::file_ref(pointer);
    assert_eq!(
        crate::seek::fseek(file, 0, 0),
        Err(Fail::from_int(raven_libc_abi::generated::errno::ESPIPE))
    );
    // SAFETY: the pointer comes from the Box allocated in this test (or the caller contract of the C API) and is live for the call.
    unsafe { drop(std::boxed::Box::from_raw(cookie)) };
}

#[test]
fn funopen_without_both_functions_is_einval() {
    let state: &'static crate::state::State<FakeEnv> = crate::fake_ut::leaked_state();
    let cookie = make();
    assert_eq!(
        funopen(state, cookie.cast(), None, None, None, None).map(|_| ()),
        Err(Fail::from_int(raven_libc_abi::generated::errno::EINVAL))
    );
    // SAFETY: the pointer comes from the Box allocated in this test (or the caller contract of the C API) and is live for the call.
    unsafe { drop(std::boxed::Box::from_raw(cookie)) };
}

#[test]
fn funopen_write_short_callback_is_a_short_write() {
    let state: &'static crate::state::State<FakeEnv> = crate::fake_ut::leaked_state();
    let cookie = make();
    let pointer = funopen(
        state,
        cookie.cast(),
        None,
        Some(fun_write),
        None,
        Some(fun_close),
    )
    .expect("funopen");
    let file = crate::raw::file_ref(pointer);
    crate::setvbuf::setvbuf(file, None, crate::setvbuf::IONBF, 0).expect("setvbuf");
    assert_eq!(crate::block_io::fwrite(file, b"xyz", 1, 3), Ok(3));
    // SAFETY: the pointer comes from the Box allocated in this test (or the caller contract of the C API) and is live for the call.
    unsafe { drop(std::boxed::Box::from_raw(cookie)) };
}
