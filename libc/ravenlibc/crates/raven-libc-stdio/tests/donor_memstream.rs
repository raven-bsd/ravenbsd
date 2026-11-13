// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! NetBSD `t_open_memstream.c` and the FreeBSD `open_memstream2_test.c` /
//! `open_wmemstream_test.c` expectations.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects,
    clippy::undocumented_unsafe_blocks,
    clippy::bool_assert_comparison
)]

mod support;

use raven_libc_stdio::{
    Fail, fclose, fflush, fmemopen, fputws, fseek, ftell, fwrite, open_memstream, open_wmemstream,
};

const OFFSET: i64 = 16384;

#[test]
fn netbsd_open_memstream_sequence() {
    let (state, _env) = support::state();
    let bufp = Box::into_raw(Box::new(core::ptr::null_mut::<u8>()));
    let sizep = Box::into_raw(Box::new(0usize));
    let file = raven_libc_stdio::file_from_ptr(
        open_memstream(state, bufp, sizep).expect("open_memstream"),
    );
    assert_eq!(ftell(file), Ok(0));
    assert_eq!(fflush(state, Some(file)), Ok(()));
    assert_eq!(unsafe { *sizep }, 0);
    assert!(!unsafe { *bufp }.is_null());
    assert_eq!(
        fseek(file, -6, 0),
        Err(Fail::from_int(raven_libc_abi::generated::errno::EINVAL))
    );
    assert_eq!(fseek(file, OFFSET, 0), Ok(()));
    assert_eq!(fwrite(file, b"hello", 1, 5), Ok(5));
    assert_eq!(fflush(state, Some(file)), Ok(()));
    assert_eq!(unsafe { *sizep }, OFFSET as usize + 5);
    assert_eq!(fseek(file, 0, 0), Ok(()));
    assert_eq!(fwrite(file, b"start", 1, 5), Ok(5));
    assert_eq!(fflush(state, Some(file)), Ok(()));
    assert_eq!(unsafe { *sizep }, 5);
    unsafe {
        let slice = core::slice::from_raw_parts(*bufp, 5);
        assert_eq!(&slice[..5], b"start");
    }
    assert_eq!(fseek(file, 100, 2), Ok(()));
    assert_eq!(fflush(state, Some(file)), Ok(()));
    assert_eq!(unsafe { *sizep }, OFFSET as usize + 5);
    assert_eq!(fseek(file, 8, 0), Ok(()));
    assert_eq!(ftell(file), Ok(8));
    assert_eq!(fseek(file, -1, 1), Ok(()));
    assert_eq!(ftell(file), Ok(7));
    assert_eq!(fseek(file, 5, 1), Ok(()));
    assert_eq!(fclose(file), Ok(()));
    assert_eq!(unsafe { *sizep }, 12);
    unsafe {
        drop(Box::from_raw(bufp));
        drop(Box::from_raw(sizep));
    }
}

#[test]
fn freebsd_memstream_nul_terminates_and_zero_fills() {
    let (state, _env) = support::state();
    let bufp = Box::into_raw(Box::new(core::ptr::null_mut::<u8>()));
    let sizep = Box::into_raw(Box::new(0usize));
    let file = raven_libc_stdio::file_from_ptr(
        open_memstream(state, bufp, sizep).expect("open_memstream"),
    );
    assert_eq!(fwrite(file, b"ab", 1, 2), Ok(2));
    assert_eq!(fflush(state, Some(file)), Ok(()));
    assert_eq!(fseek(file, 4, 0), Ok(()));
    assert_eq!(fwrite(file, b"cd", 1, 2), Ok(2));
    assert_eq!(fflush(state, Some(file)), Ok(()));
    unsafe {
        let size = *sizep;
        let slice = core::slice::from_raw_parts(*bufp, size + 1);
        assert_eq!(&slice[..2], b"ab");
        assert_eq!(&slice[2..4], b"\0\0");
        assert_eq!(&slice[4..6], b"cd");
        assert_eq!(slice[6], 0);
    }
    assert_eq!(fclose(file), Ok(()));
    let data = unsafe { *bufp };
    let size = unsafe { *sizep };
    unsafe {
        drop(Box::from_raw(core::ptr::slice_from_raw_parts_mut(
            data, size,
        )));
        drop(Box::from_raw(bufp));
        drop(Box::from_raw(sizep));
    }
}

#[test]
fn freebsd_memstream_rejects_null_outputs() {
    let (state, _env) = support::state();
    assert_eq!(
        open_memstream(state, core::ptr::null_mut(), core::ptr::null_mut()).map(|_| ()),
        Err(Fail::from_int(raven_libc_abi::generated::errno::EINVAL))
    );
    assert_eq!(
        open_wmemstream(state, core::ptr::null_mut(), core::ptr::null_mut()).map(|_| ()),
        Err(Fail::from_int(raven_libc_abi::generated::errno::EINVAL))
    );
}

#[test]
fn freebsd_wmemstream_wide_text_and_size() {
    let (state, _env) = support::state();
    let bufp = Box::into_raw(Box::new(core::ptr::null_mut::<u32>()));
    let sizep = Box::into_raw(Box::new(0usize));
    let file = raven_libc_stdio::file_from_ptr(
        open_wmemstream(state, bufp, sizep).expect("open_wmemstream"),
    );
    assert_eq!(
        fputws(file, &[u32::from(b'h'), u32::from(b'i'), 0x20ac]),
        Ok(())
    );
    assert_eq!(unsafe { *sizep }, 3);
    assert_eq!(ftell(file), Ok(3));
    assert_eq!(fseek(file, 0, 0), Ok(()));
    unsafe {
        let data = *bufp;
        assert_eq!(*data, u32::from(b'h'));
        assert_eq!(*data.add(1), u32::from(b'i'));
        assert_eq!(*data.add(2), 0x20ac);
        drop(Box::from_raw(bufp));
        drop(Box::from_raw(sizep));
    }
}

#[test]
fn freebsd_fmemopen2_modes() {
    let (state, _env) = support::state();
    let room = Box::leak(Box::new([b'a'; 8]));
    let file = raven_libc_stdio::file_from_ptr(
        fmemopen(state, Some((room.as_mut_ptr(), 8)), 8, b"r+").expect("fmemopen"),
    );
    assert_eq!(fseek(file, 0, 2), Ok(()));
    assert_eq!(ftell(file), Ok(8));
    assert_eq!(fseek(file, 0, 0), Ok(()));
    assert_eq!(fwrite(file, b"XY", 1, 2), Ok(2));
    assert_eq!(fflush(state, Some(file)), Ok(()));
    assert_eq!(&room[..4], b"XYaa");
    let _ = fclose(file);
}

#[test]
fn freebsd_fmemopen_binary_keeps_bytes() {
    let (state, _env) = support::state();
    let room = Box::leak(Box::new([0u8; 8]));
    room[0] = b'A';
    room[1] = 0;
    room[2] = b'B';
    let file = raven_libc_stdio::file_from_ptr(
        fmemopen(state, Some((room.as_mut_ptr(), 8)), 8, b"rb").expect("fmemopen"),
    );
    assert_eq!(raven_libc_stdio::fgetc(file), Ok(i32::from(b'A')));
    assert_eq!(raven_libc_stdio::fgetc(file), Ok(0));
    assert_eq!(raven_libc_stdio::fgetc(file), Ok(i32::from(b'B')));
    let _ = fclose(file);
}
