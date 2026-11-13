// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! Unit tests for [`crate::close`]: flush-then-close, list removal, `fcloseall` and the
//! caller-allocated FILE form.
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
use crate::fake_ut::{FakeEnv, leaked_file, leaked_state, leaked_streams, open_file};

#[test]
fn fclose_flushes_and_removes_the_stream() {
    let state = leaked_state();
    state.env().add_file(b"/f", b"", 1024);
    let file = open_file(state, b"/f", b"w");
    assert_eq!(crate::block_io::fwrite(file, b"abc", 1, 3), Ok(3));
    assert!(state.ofl_head().is_some());
    let pointer = (file as *const File<FakeEnv>).cast_mut();
    assert_eq!(fclose(file), Ok(()));
    assert_eq!(state.env().contents(b"/f"), b"abc".to_vec());
    assert!(state.ofl_head().is_none());
    let _ = pointer;
}

#[test]
fn fclose_of_a_standard_stream_does_not_free_or_unlink() {
    let state = leaked_state();
    state.env().install_std_file();
    let streams = leaked_streams(state);
    let stdout = streams.stdout();
    let pointer = (stdout as *const File<FakeEnv>).cast_mut();
    assert_eq!(fclose(stdout), Ok(()));
    let restored = crate::raw::file_ref(pointer);
    assert!(restored.is_perm());
    assert_eq!(restored.fd(), 1);
}

#[test]
fn fcloseall_leaves_the_standard_streams() {
    let state = leaked_state();
    state.env().install_std_file();
    let streams = leaked_streams(state);
    let _stdin = streams.stdin();
    let _stdout = streams.stdout();
    state.env().add_file(b"/a", b"", 1024);
    state.env().add_file(b"/b", b"", 1024);
    let a = open_file(state, b"/a", b"w");
    let b = open_file(state, b"/b", b"w");
    assert_eq!(crate::block_io::fwrite(a, b"a", 1, 1), Ok(1));
    assert_eq!(crate::block_io::fwrite(b, b"b", 1, 1), Ok(1));
    assert_eq!(fcloseall(state), Ok(()));
    assert_eq!(state.env().contents(b"/a"), b"a".to_vec());
    assert_eq!(state.env().contents(b"/b"), b"b".to_vec());
    assert!(state.ofl_head().is_none());
}

#[test]
fn fclose_ca_only_closes_the_backend() {
    let state = leaked_state();
    state.env().add_file(b"/f", b"x", 1024);
    let file = leaked_file(state);
    let buffer = std::boxed::Box::leak(std::boxed::Box::new([0u8; 64]));
    crate::open::fopen_rb_ca(state, b"/f", file, (buffer.as_mut_ptr(), buffer.len()))
        .expect("fopen_rb_ca");
    assert_eq!(crate::char_io::getc(file), Ok(i32::from(b'x')));
    assert_eq!(fclose_ca(file), Ok(()));
}
