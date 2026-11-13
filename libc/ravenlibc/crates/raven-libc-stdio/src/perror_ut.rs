// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! Unit tests for [`crate::perror`]: one gathering write, the exact bytes and the orientation
//! restore (R48).
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
use crate::fake_ut::{Op, leaked_state, leaked_streams};

#[test]
fn perror_writes_one_message_in_one_call() {
    let state = leaked_state();
    state.env().install_std_file();
    let streams = leaked_streams(state);
    let stderr = streams.stderr();
    stderr.st_mut().mode = 1;
    state.env().clear_trace();
    assert_eq!(perror(state, b"open", b"No such file"), Ok(()));
    assert_eq!(stderr.st().mode, 1);
    let trace = state.env().trace();
    assert_eq!(trace.len(), 1, "one writev: {trace:?}");
    assert!(matches!(trace[0], Op::Write2 { fd: 2, ret: 19, .. }));
    assert_eq!(state.env().data(2), b"open: No such file\n".to_vec());
}

#[test]
fn perror_without_a_prefix() {
    let state = leaked_state();
    state.env().install_std_file();
    let streams = leaked_streams(state);
    streams.stderr();
    assert_eq!(perror(state, b"", b"Boom"), Ok(()));
    assert_eq!(state.env().data(2), b"Boom\n".to_vec());
}

#[test]
fn perror_without_stderr_is_ebadf() {
    let state = leaked_state();
    assert_eq!(
        perror(state, b"x", b"y"),
        Err(Fail::from_int(raven_libc_abi::generated::errno::EBADF))
    );
}

#[test]
fn perror_reports_a_write_error() {
    let state = leaked_state();
    state.env().install_std_file();
    let streams = leaked_streams(state);
    streams.stderr();
    state.env().fail_write_after(1);
    assert!(perror(state, b"x", b"y").is_err());
}
