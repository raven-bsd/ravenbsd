// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! Unit tests for [`crate::line_io`]: `getdelim` growth and return rule (trap S7), `fgets`
//! truncation, `fgetln`, `fputs` and `puts`.
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
use crate::fake_ut::{FakeEnv, leaked_state, leaked_streams, open_file};

fn state_with(data: &[u8]) -> &'static crate::state::State<FakeEnv> {
    let state = leaked_state();
    state.env().add_file(b"/f", data, 1024);
    state
}

#[test]
fn getdelim_returns_length_without_the_nul() {
    let state = state_with(b"one\ntwo\nthree");
    let file = open_file(state, b"/f", b"r");
    let mut data = std::vec::Vec::new();
    let mut line = LineBuf::new(&mut data, 0);
    assert_eq!(getdelim(file, &mut line, i32::from(b'\n')), Ok(4));
    assert_eq!(line.line(), b"one\n");
    assert_eq!(line.bytes().last(), Some(&0));
    assert!(line.capacity() >= 4);
    assert_eq!(getline(file, &mut line), Ok(4));
    assert_eq!(line.line(), b"two\n");
    assert_eq!(getline(file, &mut line), Ok(5));
    assert_eq!(line.line(), b"three");
    assert_eq!(getline(file, &mut line), Err(Fail::Eof));
    assert!(crate::status::feof(file));
}

#[test]
fn getdelim_keeps_a_partial_last_line() {
    let state = state_with(b"partial");
    let file = open_file(state, b"/f", b"r");
    let mut data = std::vec::Vec::new();
    let mut line = LineBuf::new(&mut data, 0);
    assert_eq!(getdelim(file, &mut line, i32::from(b'\n')), Ok(7));
    assert_eq!(line.line(), b"partial");
    assert!(crate::status::feof(file));
}

#[test]
fn getdelim_grows_an_existing_buffer() {
    let state = state_with(b"0123456789abcdefghij\n");
    let file = open_file(state, b"/f", b"r");
    let mut data = std::vec![0u8; 4];
    let mut line = LineBuf::new(&mut data, 4);
    assert_eq!(getline(file, &mut line), Ok(21));
    assert_eq!(line.line(), b"0123456789abcdefghij\n");
    assert!(line.capacity() >= 22);
}

#[test]
fn fgets_stops_at_a_newline_and_terminates() {
    let state = state_with(b"abc\ndef");
    let file = open_file(state, b"/f", b"r");
    let mut buffer = [0u8; 8];
    assert_eq!(fgets(file, &mut buffer), Ok(4));
    assert_eq!(&buffer[..5], b"abc\n\0");
    assert_eq!(fgets(file, &mut buffer), Ok(3));
    assert_eq!(&buffer[..4], b"def\0");
    assert_eq!(fgets(file, &mut buffer), Err(Fail::Eof));
}

#[test]
fn fgets_truncates_without_the_newline() {
    let state = state_with(b"abcdef\n");
    let file = open_file(state, b"/f", b"r");
    let mut buffer = [0u8; 4];
    assert_eq!(fgets(file, &mut buffer), Ok(3));
    assert_eq!(&buffer, b"abc\0");
    assert_eq!(fgets(file, &mut buffer), Ok(3));
    assert_eq!(&buffer, b"def\0");
}

#[test]
fn fgets_size_one_is_an_empty_string() {
    let state = state_with(b"x");
    let file = open_file(state, b"/f", b"r");
    let mut buffer = [0u8; 1];
    assert_eq!(fgets(file, &mut buffer), Ok(0));
    assert_eq!(buffer, [0u8]);
}

#[test]
fn fputs_and_puts_write_the_text() {
    let state = leaked_state();
    state.env().add_file(b"/f", b"", 1024);
    let file = open_file(state, b"/f", b"w");
    assert_eq!(fputs(file, b"hello"), Ok(()));
    assert_eq!(fputs(file, b" world"), Ok(()));
    crate::flush::fflush(state, Some(file)).expect("fflush");
    assert_eq!(state.env().contents(b"/f"), b"hello world".to_vec());
    state.env().install_std_file();
    let streams = leaked_streams(state);
    let stdout = streams.stdout();
    assert_eq!(puts(state, b"line"), Ok(()));
    crate::flush::fflush(state, Some(stdout)).expect("fflush stdout");
    assert_eq!(state.env().data(1), b"line\n".to_vec());
}

#[test]
fn fgetln_returns_the_line_with_newline() {
    let state = state_with(b"first\nsecond\n");
    let file = open_file(state, b"/f", b"r");
    assert_eq!(fgetln(file), Ok(b"first\n".to_vec()));
    assert_eq!(fgetln(file), Ok(b"second\n".to_vec()));
    assert!(file.st().getln_buf.is_some());
    assert_eq!(fgetln(file), Err(Fail::Eof));
}

#[test]
fn fgetln_long_line_uses_the_growing_buffer() {
    let data = std::vec![b'x'; 5000];
    let state = state_with(&data);
    let file = open_file(state, b"/f", b"r");
    let line = fgetln(file).expect("fgetln");
    assert_eq!(line.len(), 5000);
    assert!(line.iter().all(|byte| *byte == b'x'));
}
