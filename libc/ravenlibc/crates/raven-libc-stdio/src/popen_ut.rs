// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! Unit tests for [`crate::popen`]: the stream half and the sibling-pipe scan (A-stdio-16).
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
use crate::traits::SysFile;

#[test]
fn from_pipe_stores_the_pid_and_mode() {
    let state = leaked_state();
    let (read, _write) = state.env().pipe2(0).expect("pipe2");
    let file = crate::raw::file_ref(File::from_pipe(state, read, 4242, b"r").expect("from_pipe"));
    assert_eq!(file.pipe_pid(), 4242);
    assert_eq!(check_pipe(file), Ok(4242));
}

#[test]
fn from_pipe_rejects_a_bad_mode() {
    let state = leaked_state();
    let (read, write) = state.env().pipe2(0).expect("pipe2");
    assert_eq!(
        File::<FakeEnv>::from_pipe(state, read, 1, b"q").map(|_| ()),
        Err(Fail::from_int(raven_libc_abi::generated::errno::EINVAL))
    );
    let _ = write;
}

#[test]
fn close_sibling_pipes_finds_exactly_the_popen_streams() {
    let state = leaked_state();
    state.env().add_file(b"/f", b"", 1024);
    let ordinary = open_file(state, b"/f", b"w");
    assert_eq!(ordinary.pipe_pid(), 0);
    let (r1, w1) = state.env().pipe2(0).expect("pipe2");
    let first = crate::raw::file_ref(File::from_pipe(state, r1, 10, b"r").expect("pipe1"));
    let (r2, w2) = state.env().pipe2(0).expect("pipe2");
    let second = crate::raw::file_ref(File::from_pipe(state, w2, 11, b"w").expect("pipe2"));
    let _ = w1;
    let mut fds = close_sibling_pipes(state);
    fds.sort_unstable();
    let mut expected = [first.fd(), second.fd()];
    expected.sort_unstable();
    assert_eq!(fds, expected.to_vec());
    assert_eq!(
        check_pipe(ordinary),
        Err(Fail::from_int(raven_libc_abi::generated::errno::EINVAL))
    );
    assert_eq!(check_pipe(first), Ok(10));
    let _ = r2;
}
