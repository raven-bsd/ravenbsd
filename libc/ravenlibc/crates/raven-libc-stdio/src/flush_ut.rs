// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! Unit tests for [`crate::flush`], including `fflush(NULL)`'s writers-first, one-lock-at-a-time
//! walk and the first-error rule (A-stdio-7).
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
use crate::fake_ut::{FakeEnv, Op, leaked_state, leaked_streams, open_file};

fn writer_state() -> (
    &'static crate::state::State<FakeEnv>,
    &'static File<FakeEnv>,
) {
    let state = leaked_state();
    state.env().add_file(b"/f", b"", 1024);
    let file = open_file(state, b"/f", b"w");
    (state, file)
}

#[test]
fn fflush_of_one_stream_writes_pending_output() {
    let (state, file) = writer_state();
    assert_eq!(crate::block_io::fwrite(file, b"abc", 1, 3), Ok(3));
    assert_eq!(fflush(state, Some(file)), Ok(()));
    assert_eq!(state.env().contents(b"/f"), b"abc".to_vec());
    assert!(!file.st().has_pending_write());
}

#[test]
fn fflush_of_a_reader_repositions_the_descriptor() {
    let state = leaked_state();
    state.env().add_file(b"/f", b"abcdef", 1024);
    let file = open_file(state, b"/f", b"r");
    assert_eq!(crate::char_io::getc(file), Ok(i32::from(b'a')));
    assert_eq!(crate::char_io::getc(file), Ok(i32::from(b'b')));
    assert_eq!(file.st().unread(), 4);
    state.env().clear_trace();
    assert_eq!(fflush(state, Some(file)), Ok(()));
    let trace = state.env().trace();
    let seek = trace.iter().find_map(|op| match op {
        Op::Seek { off, whence, .. } => Some((*off, *whence)),
        _ => None,
    });
    assert!(
        seek.is_some(),
        "fflush must reposition a read stream: {trace:?}"
    );
    assert_eq!(crate::seek::ftell(file), Ok(2));
}

#[test]
fn fflush_all_writes_the_standard_streams_and_the_list() {
    let state = leaked_state();
    state.env().install_std_file();
    let streams = leaked_streams(state);
    let stdout = streams.stdout();
    let stderr = streams.stderr();
    assert_eq!(crate::block_io::fwrite(stdout, b"out", 1, 3), Ok(3));
    assert_eq!(crate::block_io::fwrite(stderr, b"err", 1, 3), Ok(3));
    assert_eq!(fflush(state, None), Ok(()));
    assert_eq!(state.env().data(1), b"out".to_vec());
    assert_eq!(state.env().data(2), b"err".to_vec());
}

#[test]
fn first_error_is_returned_and_the_rest_is_flushed() {
    let state = leaked_state();
    state.env().add_file(b"/a", b"", 1024);
    state.env().add_file(b"/b", b"", 1024);
    let first = open_file(state, b"/a", b"w");
    let second = open_file(state, b"/b", b"w");
    assert_eq!(crate::block_io::fwrite(first, b"1", 1, 1), Ok(1));
    assert_eq!(crate::block_io::fwrite(second, b"2", 1, 1), Ok(1));
    state.env().fail_write_after(1);
    assert!(fflush(state, None).is_err());
    assert_eq!(state.env().contents(b"/a"), b"1".to_vec());
}

#[test]
fn fpurge_drops_without_writing() {
    let (state, file) = writer_state();
    assert_eq!(crate::block_io::fwrite(file, b"abc", 1, 3), Ok(3));
    assert_eq!(fpurge(file), Ok(()));
    assert!(!file.st().has_pending_write());
    assert_eq!(fflush(state, Some(file)), Ok(()));
    assert!(state.env().contents(b"/f").is_empty());
}
