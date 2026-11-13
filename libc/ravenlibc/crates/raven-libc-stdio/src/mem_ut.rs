// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! Unit tests for [`crate::mem`]: the NUL rules of the three memory streams, ported from the
//! NetBSD/FreeBSD/OpenBSD donor tests (`t_fmemopen.c`, `t_open_memstream.c`,
//! `open_memstream2_test.c`, `open_wmemstream_test.c`, `fmemopentest.c`); the expected values
//! are the donors'.

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

use super::*;
use crate::fake_ut::FakeEnv;

fn state() -> &'static crate::state::State<FakeEnv> {
    crate::fake_ut::leaked_state()
}

fn room<const N: usize>() -> &'static mut [u8; N] {
    std::boxed::Box::leak(std::boxed::Box::new([0u8; N]))
}

/// Hand the region to the stream as the C `(void *, size_t)` pair.
fn pair<const N: usize>(room: &mut [u8; N]) -> (*mut u8, usize) {
    (room.as_mut_ptr(), N)
}

/// Read the region after the stream wrote through the raw pointer.
fn view<const N: usize>(room: &[u8; N]) -> &[u8] {
    // SAFETY: the pointer comes from the Box allocated in this test (or the caller contract of the C API) and is live for the call.
    unsafe { core::slice::from_raw_parts(room.as_ptr(), N) }
}

#[test]
fn fmemopen_write_mode_has_no_read_side() {
    let state = state();
    let room = room::<16>();
    let file = crate::raw::file_ref(fmemopen(state, Some(pair(room)), 16, b"w").expect("fmemopen"));
    assert_eq!(crate::block_io::fwrite(file, b"abc", 1, 3), Ok(3));
    crate::flush::fflush(state, Some(file)).expect("fflush");
    assert_eq!(&view(room)[..3], b"abc");
    assert_eq!(view(room)[3], 0);
    assert_eq!(
        crate::char_io::getc(file),
        Err(Fail::from_int(raven_libc_abi::generated::errno::EBADF))
    );
}

#[test]
fn fmemopen_read_mode_exposes_the_region() {
    let state = state();
    let room = room::<4>();
    room.copy_from_slice(b"abcd");
    let file = crate::raw::file_ref(fmemopen(state, Some(pair(room)), 4, b"r").expect("fmemopen"));
    assert_eq!(crate::char_io::getc(file), Ok(i32::from(b'a')));
    assert_eq!(crate::char_io::getc(file), Ok(i32::from(b'b')));
    assert_eq!(crate::char_io::getc(file), Ok(i32::from(b'c')));
    assert_eq!(crate::char_io::getc(file), Ok(i32::from(b'd')));
    assert_eq!(crate::char_io::getc(file), Err(Fail::Eof));
    let buffer = *b"x";
    assert_eq!(crate::block_io::fwrite(file, &buffer, 1, 1), Ok(0));
    assert!(crate::status::ferror(file));
}

#[test]
fn fmemopen_append_starts_at_the_first_nul() {
    let state = state();
    let room = room::<16>();
    room[..3].copy_from_slice(b"abc");
    let file = crate::raw::file_ref(fmemopen(state, Some(pair(room)), 16, b"a").expect("fmemopen"));
    assert_eq!(crate::seek::ftell(file), Ok(3));
    assert_eq!(crate::block_io::fwrite(file, b"de", 1, 2), Ok(2));
    crate::flush::fflush(state, Some(file)).expect("fflush");
    assert_eq!(&view(room)[..6], b"abcde\0");
}

#[test]
fn fmemopen_write_plus_starts_empty_and_keeps_the_nul() {
    let state = state();
    let room = room::<4>();
    room.copy_from_slice(b"ZZZZ");
    let file = crate::raw::file_ref(fmemopen(state, Some(pair(room)), 4, b"w+").expect("fmemopen"));
    assert_eq!(view(room)[0], 0);
    assert_eq!(crate::block_io::fwrite(file, b"xy", 1, 2), Ok(2));
    crate::flush::fflush(state, Some(file)).expect("fflush");
    assert_eq!(&view(room)[..3], b"xy\0");
    assert_eq!(crate::seek::fseek(file, 0, 0), Ok(()));
    assert_eq!(crate::char_io::getc(file), Ok(i32::from(b'x')));
    assert_eq!(crate::char_io::getc(file), Ok(i32::from(b'y')));
    assert_eq!(crate::char_io::getc(file), Err(Fail::Eof));
    assert_eq!(view(room)[2], 0);
}

#[test]
fn fmemopen_seek_bounds() {
    let state = state();
    let room = room::<8>();
    room.copy_from_slice(b"abcdefgh");
    let file = crate::raw::file_ref(fmemopen(state, Some(pair(room)), 8, b"r+").expect("fmemopen"));
    assert_eq!(crate::seek::fseek(file, 3, 0), Ok(()));
    assert_eq!(crate::seek::ftell(file), Ok(3));
    assert_eq!(crate::seek::fseek(file, -1, 1), Ok(()));
    assert_eq!(crate::seek::ftell(file), Ok(2));
    assert_eq!(crate::seek::fseek(file, 0, 2), Ok(()));
    assert_eq!(crate::seek::ftell(file), Ok(8));
    assert_eq!(crate::seek::fseek(file, -1, 1), Ok(()));
    assert_eq!(
        crate::seek::fseek(file, -100, 1),
        Err(Fail::from_int(raven_libc_abi::generated::errno::EINVAL))
    );
    assert_eq!(
        crate::seek::fseek(file, 9, 0),
        Err(Fail::from_int(raven_libc_abi::generated::errno::EINVAL))
    );
    assert_eq!(
        crate::seek::fseek(file, 0, 9),
        Err(Fail::from_int(raven_libc_abi::generated::errno::EINVAL))
    );
}

#[test]
fn fmemopen_with_no_buffer_allocates_zeroed_memory() {
    let state = state();
    let pointer = fmemopen(state, None, 8, b"a+").expect("fmemopen");
    let file = crate::raw::file_ref(pointer);
    assert_eq!(crate::seek::ftell(file), Ok(0));
    assert_eq!(crate::block_io::fwrite(file, b"hi", 1, 2), Ok(2));
    crate::flush::fflush(state, Some(file)).expect("fflush");
}

#[test]
fn open_memstream_matches_the_netbsd_test() {
    const OFFSET: i64 = 16384;
    let state = state();
    let buf_ptr = std::boxed::Box::into_raw(std::boxed::Box::new(core::ptr::null_mut::<u8>()));
    let size_ptr = std::boxed::Box::into_raw(std::boxed::Box::new(0usize));
    let pointer = open_memstream(state, buf_ptr, size_ptr).expect("open_memstream");
    let file = crate::raw::file_ref(pointer);
    assert_eq!(crate::seek::ftello(file), Ok(0));
    assert_eq!(crate::flush::fflush(state, Some(file)), Ok(()));
    // SAFETY: the pointer comes from the Box allocated in this test (or the caller contract of the C API) and is live for the call.
    assert_eq!(unsafe { *size_ptr }, 0);
    // SAFETY: the pointer comes from the Box allocated in this test (or the caller contract of the C API) and is live for the call.
    assert!(!unsafe { *buf_ptr }.is_null());
    assert_eq!(
        crate::seek::fseek(file, -6, 0),
        Err(Fail::from_int(raven_libc_abi::generated::errno::EINVAL))
    );
    assert_eq!(crate::seek::fseek(file, OFFSET, 0), Ok(()));
    assert_eq!(crate::block_io::fwrite(file, b"hello", 1, 5), Ok(5));
    assert_eq!(crate::flush::fflush(state, Some(file)), Ok(()));
    // SAFETY: the pointer comes from the Box allocated in this test (or the caller contract of the C API) and is live for the call.
    assert_eq!(unsafe { *size_ptr }, OFFSET as usize + 5);
    assert_eq!(crate::seek::fseek(file, 0, 0), Ok(()));
    assert_eq!(crate::block_io::fwrite(file, b"start", 1, 5), Ok(5));
    assert_eq!(crate::flush::fflush(state, Some(file)), Ok(()));
    // SAFETY: the pointer comes from the Box allocated in this test (or the caller contract of the C API) and is live for the call.
    assert_eq!(unsafe { *size_ptr }, 5);
    // SAFETY: the pointer comes from the Box allocated in this test (or the caller contract of the C API) and is live for the call.
    unsafe {
        let slice = core::slice::from_raw_parts(*buf_ptr, 5);
        assert_eq!(&slice[..5], b"start");
    }
    assert_eq!(crate::seek::fseek(file, 100, 2), Ok(()));
    assert_eq!(crate::flush::fflush(state, Some(file)), Ok(()));
    // SAFETY: the pointer comes from the Box allocated in this test (or the caller contract of the C API) and is live for the call.
    assert_eq!(unsafe { *size_ptr }, OFFSET as usize + 5);
    assert_eq!(crate::seek::fseek(file, 8, 0), Ok(()));
    assert_eq!(crate::seek::ftell(file), Ok(8));
    assert_eq!(crate::seek::fseek(file, -1, 1), Ok(()));
    assert_eq!(crate::seek::ftell(file), Ok(7));
    assert_eq!(crate::seek::fseek(file, 5, 1), Ok(()));
    assert_eq!(crate::close::fclose(file), Ok(()));
    // SAFETY: the pointer comes from the Box allocated in this test (or the caller contract of the C API) and is live for the call.
    assert_eq!(unsafe { *size_ptr }, 12);
    // SAFETY: the pointer comes from the Box allocated in this test (or the caller contract of the C API) and is live for the call.
    unsafe {
        drop(std::boxed::Box::from_raw(buf_ptr));
        drop(std::boxed::Box::from_raw(size_ptr));
    }
}

#[test]
fn open_wmemstream_is_born_wide() {
    let state = state();
    let buf_ptr = std::boxed::Box::into_raw(std::boxed::Box::new(core::ptr::null_mut::<u32>()));
    let size_ptr = std::boxed::Box::into_raw(std::boxed::Box::new(0usize));
    let pointer = open_wmemstream(state, buf_ptr, size_ptr).expect("open_wmemstream");
    let file = crate::raw::file_ref(pointer);
    assert_eq!(crate::wide::fwide(file, 0), 1);
    assert_eq!(
        crate::wide::fputws(file, &[u32::from(b'h'), u32::from(b'i'), 0x20ac]),
        Ok(())
    );
    // SAFETY: the pointer comes from the Box allocated in this test (or the caller contract of the C API) and is live for the call.
    assert_eq!(unsafe { *size_ptr }, 3);
    assert_eq!(crate::seek::ftell(file), Ok(3));
    assert_eq!(crate::seek::fseek(file, 0, 0), Ok(()));
    // SAFETY: the pointer comes from the Box allocated in this test (or the caller contract of the C API) and is live for the call.
    assert_eq!(unsafe { *size_ptr }, 0);
    // SAFETY: the pointer comes from the Box allocated in this test (or the caller contract of the C API) and is live for the call.
    unsafe {
        let buf = *buf_ptr;
        assert_eq!(*buf, u32::from(b'h'));
        assert_eq!(*buf.add(1), u32::from(b'i'));
        assert_eq!(*buf.add(2), 0x20ac);
        drop(std::boxed::Box::from_raw(buf_ptr));
        drop(std::boxed::Box::from_raw(size_ptr));
    }
}
