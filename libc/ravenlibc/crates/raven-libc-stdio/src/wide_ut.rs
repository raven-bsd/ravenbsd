// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! Unit tests for [`crate::wide`]: orientation (T16), UTF-8 decoding, `ungetwc`, invalid
//! sequences and the C-locale byte mode.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects,
    clippy::bool_assert_comparison,
    clippy::undocumented_unsafe_blocks,
    clippy::uninlined_format_args
)]

use super::*;
use crate::fake_ut::{FakeEnv, leaked_state, open_file};

fn reader(
    data: &[u8],
) -> (
    &'static crate::state::State<FakeEnv>,
    &'static File<FakeEnv>,
) {
    let state = leaked_state();
    state.env().add_file(b"/f", data, 1024);
    let file = open_file(state, b"/f", b"r");
    (state, file)
}

#[test]
fn fwide_queries_and_sets_once() {
    let (_, file) = reader(b"x");
    assert_eq!(fwide(file, 0), 0);
    assert_eq!(fwide(file, 1), 1);
    assert_eq!(fwide(file, -1), 1);
    assert_eq!(fwide(file, 0), 1);
    file.st_mut().mode = 0;
    assert_eq!(fwide(file, -1), -1);
    assert_eq!(fwide(file, 1), -1);
}

#[test]
fn fgetwc_decodes_ascii_and_utf8() {
    let (_, file) = reader("aé€".as_bytes());
    assert_eq!(fgetwc(file), Ok(u32::from(b'a')));
    assert_eq!(fgetwc(file), Ok(0xe9));
    assert_eq!(fgetwc(file), Ok(0x20ac));
    assert_eq!(fgetwc(file), Err(Fail::Eof));
    assert_eq!(fwide(file, 0), 1);
}

#[test]
fn fputwc_encodes_and_fputws_writes_a_string() {
    let state = leaked_state();
    state.env().add_file(b"/f", b"", 1024);
    let file = open_file(state, b"/f", b"w");
    assert_eq!(fputwc(file, u32::from(b'a')), Ok(u32::from(b'a')));
    assert_eq!(fputwc(file, 0xe9), Ok(0xe9));
    assert_eq!(fputws(file, &[0x20ac, u32::from(b'!')]), Ok(()));
    crate::flush::fflush(state, Some(file)).expect("fflush");
    assert_eq!(state.env().contents(b"/f"), "aé€!".as_bytes().to_vec());
}

#[test]
fn ungetwc_pushes_the_encoded_character() {
    let (_, file) = reader("éx".as_bytes());
    assert_eq!(fgetwc(file), Ok(0xe9));
    assert_eq!(ungetwc(file, 0xe9), Ok(0xe9));
    assert_eq!(fgetwc(file), Ok(0xe9));
    assert_eq!(fgetwc(file), Ok(u32::from(b'x')));
    assert_eq!(ungetwc(file, WEOF), Err(Fail::Eof));
}

#[test]
fn invalid_utf8_sets_the_error_flag() {
    let (_, file) = reader(&[0xff, 0xfe]);
    assert_eq!(fgetwc(file), Err(Fail::Eof));
    assert_ne!(file.st().flags & F_ERR, 0);
    assert!(crate::status::ferror(file));
}

#[test]
fn mixing_orientations_fails() {
    let (_, file) = reader(b"abc");
    assert_eq!(fgetwc(file), Ok(u32::from(b'a')));
    assert_eq!(
        crate::char_io::getc(file),
        Err(Fail::from_int(raven_libc_abi::generated::errno::EBADF))
    );
    let (_, other) = reader(b"abc");
    assert_eq!(crate::char_io::getc(other), Ok(u32::from(b'a') as i32));
    assert_eq!(fputwc(other, u32::from(b'z')), Err(Fail::Eof));
    assert_ne!(other.st().flags & F_ERR, 0);
}

#[test]
fn fgetws_stops_at_a_newline_and_terminates() {
    let (_, file) = reader("ab\ncd".as_bytes());
    let mut dest = [0u32; 8];
    assert_eq!(fgetws(file, &mut dest), Ok(3));
    assert_eq!(
        &dest[..4],
        &[u32::from(b'a'), u32::from(b'b'), u32::from(b'\n'), 0]
    );
    assert_eq!(fgetws(file, &mut dest), Ok(2));
    assert_eq!(&dest[..3], &[u32::from(b'c'), u32::from(b'd'), 0]);
    assert_eq!(fgetws(file, &mut dest), Err(Fail::Eof));
}

#[test]
fn byte_mode_treats_a_high_byte_as_a_character() {
    let (_, file) = reader(&[0x80, b'a']);
    file.st_mut().mbmode = raven_libc_ctype::MbMode::Byte;
    let first = fgetwc(file).expect("byte mode");
    assert_ne!(first, WEOF);
    assert_eq!(fgetwc(file), Ok(u32::from(b'a')));
}

#[test]
fn getwc_and_putwc_alias_the_functions() {
    let (state, file) = reader("q".as_bytes());
    assert_eq!(getwc(file), Ok(u32::from(b'q')));
    let _ = state;
    let out_state = leaked_state();
    out_state.env().add_file(b"/o", b"", 1024);
    let out = open_file(out_state, b"/o", b"w");
    assert_eq!(putwc(out, u32::from(b'w')), Ok(u32::from(b'w')));
}

#[test]
fn getwchar_and_putwchar_use_the_standard_slots() {
    let state = leaked_state();
    let streams = crate::fake_ut::leaked_streams(state);
    state.env().install_std_tty();
    streams.stdin();
    streams.stdout();
    state
        .env()
        .inner
        .lock()
        .unwrap()
        .files
        .get_mut(&0)
        .unwrap()
        .data = "é".as_bytes().to_vec();
    assert_eq!(getwchar(state), Ok(0xe9));
    assert_eq!(putwchar(state, 0xe9), Ok(0xe9));
}
