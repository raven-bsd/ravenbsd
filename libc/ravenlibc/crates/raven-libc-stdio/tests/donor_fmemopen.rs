// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! NetBSD `t_fmemopen.c` (24 cases) ported: the NUL-position table, the mode matrix, the
//! truncation rules and the seek bounds. Expected values are the donor's.
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
    Fail, clearerr, fclose, feof, fflush, fgetc, fmemopen, fputc, fread, fseek, ftell, fwrite,
    setvbuf,
};

fn region<const N: usize>(data: &[u8]) -> &'static mut [u8; N] {
    let buffer = Box::leak(Box::new([0u8; N]));
    buffer[..data.len()].copy_from_slice(data);
    buffer
}

#[test]
fn netbsd_nul_position_table_read() {
    for position in 0..43usize {
        let (state, _env) = support::state();
        let mut data = vec![b'T'; 44];
        data[position] = 0;
        let boxed = Box::leak(data.into_boxed_slice());
        let file = raven_libc_stdio::file_from_ptr(
            fmemopen(state, Some((boxed.as_mut_ptr(), 44)), 44, b"r").expect("fmemopen"),
        );
        let mut read = 0usize;
        loop {
            match fgetc(file) {
                Ok(0) => break,
                Ok(value) => {
                    assert_eq!(value as u8, b'T');
                    read += 1;
                }
                Err(Fail::Eof) => break,
                Err(Fail::Errno(error)) => panic!("errno {error:?}"),
            }
        }
        assert_eq!(read, position, "NUL at {position}");
        let _ = fclose(file);
    }
}

#[test]
fn netbsd_mode_matrix_r() {
    for mode in ["r", "rb", "r+", "rb+", "r+b"] {
        let (state, _env) = support::state();
        let room = region::<16>(b"abcdef");
        let file = raven_libc_stdio::file_from_ptr(
            fmemopen(state, Some((room.as_mut_ptr(), 16)), 16, mode.as_bytes()).expect("fmemopen"),
        );
        assert_eq!(fgetc(file), Ok(i32::from(b'a')));
        assert_eq!(ftell(file), Ok(1));
        assert_eq!(fseek(file, 0, 0), Ok(()));
        assert_eq!(fgetc(file), Ok(i32::from(b'a')));
        let _ = fclose(file);
    }
}

#[test]
fn netbsd_mode_matrix_w() {
    for mode in ["w", "wb", "w+", "wb+", "w+b"] {
        let (state, _env) = support::state();
        let room = region::<16>(b"");
        let file = raven_libc_stdio::file_from_ptr(
            fmemopen(state, Some((room.as_mut_ptr(), 16)), 16, mode.as_bytes()).expect("fmemopen"),
        );
        assert_eq!(fwrite(file, b"abc", 1, 3), Ok(3));
        assert_eq!(fflush(state, Some(file)), Ok(()));
        assert_eq!(&room[..3], b"abc");
        assert_eq!(room[3], 0);
        let _ = fclose(file);
    }
}

#[test]
fn netbsd_mode_matrix_a() {
    for mode in ["a", "ab", "a+", "ab+", "a+b"] {
        let (state, _env) = support::state();
        let room = region::<16>(b"xyz");
        let file = raven_libc_stdio::file_from_ptr(
            fmemopen(state, Some((room.as_mut_ptr(), 16)), 16, mode.as_bytes()).expect("fmemopen"),
        );
        assert_eq!(ftell(file), Ok(3));
        assert_eq!(fputc(file, i32::from(b'!')), Ok(i32::from(b'!')));
        assert_eq!(fflush(state, Some(file)), Ok(()));
        assert_eq!(&room[..4], b"xyz!");
        let _ = fclose(file);
    }
}

#[test]
fn netbsd_write_mode_truncates_at_open() {
    let (state, _env) = support::state();
    let room = region::<8>(b"abcdefg");
    let file = raven_libc_stdio::file_from_ptr(
        fmemopen(state, Some((room.as_mut_ptr(), 8)), 8, b"w").expect("fmemopen"),
    );
    assert_eq!(fwrite(file, b"12", 1, 2), Ok(2));
    assert_eq!(fflush(state, Some(file)), Ok(()));
    assert_eq!(&room[..3], b"12\0");
    let _ = fclose(file);
}

#[test]
fn netbsd_append_starts_at_nul() {
    let (state, _env) = support::state();
    let room = region::<8>(b"ab\0cd");
    let file = raven_libc_stdio::file_from_ptr(
        fmemopen(state, Some((room.as_mut_ptr(), 8)), 8, b"a+").expect("fmemopen"),
    );
    assert_eq!(ftell(file), Ok(2));
    assert_eq!(fwrite(file, b"XY", 1, 2), Ok(2));
    assert_eq!(fflush(state, Some(file)), Ok(()));
    assert_eq!(&room[..5], b"abXY\0");
    let _ = fclose(file);
}

#[test]
fn netbsd_seek_bounds_and_end() {
    let (state, _env) = support::state();
    let room = region::<10>(b"0123456789");
    let file = raven_libc_stdio::file_from_ptr(
        fmemopen(state, Some((room.as_mut_ptr(), 10)), 10, b"r+").expect("fmemopen"),
    );
    assert_eq!(fseek(file, 0, 2), Ok(()));
    assert_eq!(ftell(file), Ok(10));
    assert_eq!(fseek(file, -10, 1), Ok(()));
    assert_eq!(ftell(file), Ok(0));
    assert_eq!(
        fseek(file, 11, 0),
        Err(Fail::from_int(raven_libc_abi::generated::errno::EINVAL))
    );
    assert_eq!(
        fseek(file, -1, 1),
        Err(Fail::from_int(raven_libc_abi::generated::errno::EINVAL))
    );
    let _ = fclose(file);
}

#[test]
fn netbsd_zero_size_stream() {
    let (state, _env) = support::state();
    let room = region::<1>(b"");
    let file = raven_libc_stdio::file_from_ptr(
        fmemopen(state, Some((room.as_mut_ptr(), 0)), 0, b"r").expect("fmemopen"),
    );
    assert_eq!(fgetc(file), Err(Fail::Eof));
    assert_eq!(fread(file, &mut [0u8; 4], 1, 4), Ok(0));
    clearerr(file);
    assert!(!feof(file));
    let _ = fclose(file);
}

#[test]
fn netbsd_read_mode_exposes_the_region_length() {
    let (state, _env) = support::state();
    let room = region::<6>(b"abc");
    let file = raven_libc_stdio::file_from_ptr(
        fmemopen(state, Some((room.as_mut_ptr(), 6)), 6, b"r").expect("fmemopen"),
    );
    let mut buffer = [0u8; 8];
    assert_eq!(fread(file, &mut buffer, 1, 8), Ok(6));
    assert_eq!(&buffer[..6], b"abc\0\0\0");
    let _ = fclose(file);
}

#[test]
fn netbsd_setvbuf_none_keeps_the_stream_unbuffered() {
    let (state, _env) = support::state();
    let room = region::<8>(b"");
    let file = raven_libc_stdio::file_from_ptr(
        fmemopen(state, Some((room.as_mut_ptr(), 8)), 8, b"w").expect("fmemopen"),
    );
    assert_eq!(
        setvbuf(file, None, raven_libc_stdio::setvbuf::IONBF, 0),
        Ok(())
    );
    assert_eq!(fputc(file, i32::from(b'z')), Ok(i32::from(b'z')));
    assert_eq!(&room[..1], b"z");
    let _ = fclose(file);
}
