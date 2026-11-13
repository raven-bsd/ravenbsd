// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! Unit tests for [`crate::ungetc`]: the eight-byte limit, `ftell`/`fseek` interaction
//! (A-stdio-5), `F_EOF` clearing and the write-only rule (R37).
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

fn state_with(data: &[u8]) -> &'static crate::state::State<FakeEnv> {
    let state = leaked_state();
    state.env().add_file(b"/f", data, 1024);
    state
}

#[test]
fn eight_pushbacks_succeed_and_the_ninth_fails() {
    let state = state_with(b"abcdefghij");
    let file = open_file(state, b"/f", b"r");
    crate::mode::toread(file).expect("toread");
    {
        let st = file.st_mut();
        st.rpos = crate::file::UNGET;
        st.rend = crate::file::UNGET;
    }
    for byte in b"12345678" {
        assert_eq!(ungetc(file, i32::from(*byte)), Ok(i32::from(*byte)));
    }
    assert_eq!(ungetc(file, i32::from(b'9')), Err(Fail::Eof));
    for byte in b"87654321" {
        assert_eq!(crate::char_io::getc(file), Ok(i32::from(*byte)));
    }
    assert_eq!(crate::char_io::getc(file), Ok(i32::from(b'a')));
    file.check_invariants();
}

#[test]
fn ungetc_eof_is_a_noop() {
    let state = state_with(b"a");
    let file = open_file(state, b"/f", b"r");
    assert_eq!(ungetc(file, EOF), Err(Fail::Eof));
    assert_eq!(crate::char_io::getc(file), Ok(i32::from(b'a')));
}

#[test]
fn ftell_moves_back_by_the_pushed_byte() {
    let state = state_with(b"abcdef");
    let file = open_file(state, b"/f", b"r");
    assert_eq!(crate::char_io::getc(file), Ok(i32::from(b'a')));
    assert_eq!(crate::seek::ftell(file), Ok(1));
    assert_eq!(ungetc(file, i32::from(b'a')), Ok(i32::from(b'a')));
    assert_eq!(crate::seek::ftell(file), Ok(0));
    assert_eq!(crate::char_io::getc(file), Ok(i32::from(b'a')));
    assert_eq!(crate::seek::ftell(file), Ok(1));
}

#[test]
fn seek_cur_discards_the_pushed_byte() {
    let state = state_with(b"abcdef");
    let file = open_file(state, b"/f", b"r");
    assert_eq!(crate::char_io::getc(file), Ok(i32::from(b'a')));
    assert_eq!(ungetc(file, i32::from(b'a')), Ok(i32::from(b'a')));
    assert_eq!(crate::seek::fseek(file, 0, 1), Ok(()));
    assert_eq!(crate::seek::ftell(file), Ok(0));
    assert_eq!(crate::char_io::getc(file), Ok(i32::from(b'a')));
}

#[test]
fn ungetc_clears_eof() {
    let state = state_with(b"");
    let file = open_file(state, b"/f", b"r");
    assert_eq!(crate::char_io::getc(file), Err(Fail::Eof));
    assert!(crate::status::feof(file));
    assert_eq!(ungetc(file, i32::from(b'z')), Ok(i32::from(b'z')));
    assert!(!crate::status::feof(file));
    assert_eq!(crate::char_io::getc(file), Ok(i32::from(b'z')));
}

#[test]
fn ungetc_on_a_write_only_stream_does_not_set_ferror() {
    let state = leaked_state();
    state.env().add_file(b"/f", b"", 1024);
    let file = open_file(state, b"/f", b"w");
    assert_eq!(ungetc(file, i32::from(b'x')), Err(Fail::Eof));
    assert!(!crate::status::ferror(file));
}

#[test]
fn ungetc_pushes_the_byte_that_getc_returns() {
    let state = state_with(b"abcdef");
    let file = open_file(state, b"/f", b"r");
    for _ in 0..3 {
        let first = crate::char_io::getc(file).expect("getc");
        assert_eq!(ungetc(file, first), Ok(first));
        assert_eq!(crate::char_io::getc(file), Ok(first));
    }
    assert_eq!(crate::seek::ftell(file), Ok(3));
}
