// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! Unit tests for [`crate::seek`].
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
use crate::fake_ut::{FakeEnv, Kind, leaked_state, open_file};

fn state_with(data: &[u8]) -> &'static crate::state::State<FakeEnv> {
    let state = leaked_state();
    state.env().add_file(b"/f", data, 1024);
    state
}

#[test]
fn seek_set_cur_end_and_tell() {
    let state = state_with(b"abcdef");
    let file = open_file(state, b"/f", b"r");
    assert_eq!(fseeko(file, 2, 0), Ok(()));
    assert_eq!(ftello(file), Ok(2));
    assert_eq!(crate::char_io::getc(file), Ok(i32::from(b'c')));
    assert_eq!(ftell(file), Ok(3));
    assert_eq!(fseek(file, 0, 2), Ok(()));
    assert_eq!(ftell(file), Ok(6));
    assert_eq!(fseek(file, -2, 1), Ok(()));
    assert_eq!(ftell(file), Ok(4));
    assert_eq!(fseek(file, 0, 1), Ok(()));
    assert_eq!(ftell(file), Ok(4));
}

#[test]
fn invalid_whence_is_einval() {
    let state = state_with(b"abc");
    let file = open_file(state, b"/f", b"r");
    assert_eq!(
        fseek(file, 0, 9),
        Err(Fail::from_int(raven_libc_abi::generated::errno::EINVAL))
    );
}

#[test]
fn seek_clears_the_eof_flag() {
    let state = state_with(b"a");
    let file = open_file(state, b"/f", b"r");
    assert_eq!(crate::char_io::getc(file), Ok(i32::from(b'a')));
    assert_eq!(crate::char_io::getc(file), Err(Fail::Eof));
    assert!(crate::status::feof(file));
    assert_eq!(fseek(file, 0, 0), Ok(()));
    assert!(!crate::status::feof(file));
    assert_eq!(crate::char_io::getc(file), Ok(i32::from(b'a')));
}

#[test]
fn ftell_includes_the_pending_write() {
    let state = leaked_state();
    state.env().add_file(b"/f", b"", 1024);
    let file = open_file(state, b"/f", b"w");
    assert_eq!(crate::block_io::fwrite(file, b"abc", 1, 3), Ok(3));
    assert_eq!(ftell(file), Ok(3));
    assert_eq!(fseek(file, 0, 0), Ok(()));
    assert_eq!(ftell(file), Ok(0));
}

#[test]
fn rewind_clears_both_indicators() {
    let state = state_with(b"ab");
    let file = open_file(state, b"/f", b"r");
    assert_eq!(crate::char_io::getc(file), Ok(i32::from(b'a')));
    file.st_mut().flags |= crate::file::F_ERR;
    rewind(file);
    assert_eq!(ftell(file), Ok(0));
    assert!(!crate::status::feof(file));
    assert!(!crate::status::ferror(file));
}

#[test]
fn fgetpos_fsetpos_roundtrip() {
    let state = state_with(b"abcdef");
    let file = open_file(state, b"/f", b"r");
    assert_eq!(fseek(file, 3, 0), Ok(()));
    let pos = fgetpos(file).expect("fgetpos");
    assert_eq!(pos.off, 3);
    assert_eq!(fseek(file, 0, 0), Ok(()));
    assert_eq!(fsetpos(file, &pos), Ok(()));
    assert_eq!(ftell(file), Ok(3));
    assert_eq!(crate::char_io::getc(file), Ok(i32::from(b'd')));
}

#[test]
fn pipe_is_unseekable() {
    let state = leaked_state();
    let pipe = state.env().push_fd(b"", Kind::Pipe);
    let file = crate::open::fdopen(state, pipe, b"r").expect("fdopen");
    let file = crate::raw::file_ref(file);
    assert_eq!(
        fseeko(file, 0, 0),
        Err(Fail::from_int(raven_libc_abi::generated::errno::ESPIPE))
    );
    assert_eq!(
        ftello(file),
        Err(Fail::from_int(raven_libc_abi::generated::errno::ESPIPE))
    );
}

#[test]
fn append_write_asks_for_seek_end() {
    let state = leaked_state();
    state.env().add_file(b"/f", b"abc", 1024);
    let file = open_file(state, b"/f", b"a");
    assert_eq!(crate::block_io::fwrite(file, b"d", 1, 1), Ok(1));
    assert_eq!(ftell(file), Ok(4));
    crate::flush::fflush(state, Some(file)).expect("fflush");
    assert_eq!(state.env().contents(b"/f"), b"abcd".to_vec());
}
