// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! Unit tests for [`crate::char_io`].
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
use crate::file::{F_EOF, F_ERR};

fn state_with(data: &[u8]) -> &'static crate::state::State<FakeEnv> {
    let state = leaked_state();
    state.env().add_file(b"/f", data, 1024);
    state
}

#[test]
fn getc_reads_the_file_then_eof() {
    let state = state_with(b"ab");
    let file = open_file(state, b"/f", b"r");
    assert_eq!(getc(file), Ok(i32::from(b'a')));
    assert_eq!(getc(file), Ok(i32::from(b'b')));
    assert_eq!(getc(file), Err(Fail::Eof));
    assert_ne!(file.st().flags & F_EOF, 0);
    assert_eq!(getc(file), Err(Fail::Eof));
    file.check_invariants();
}

#[test]
fn putc_buffers_and_fflush_writes() {
    let state = leaked_state();
    state.env().add_file(b"/f", b"", 1024);
    let file = open_file(state, b"/f", b"w");
    assert_eq!(putc(file, i32::from(b'x')), Ok(i32::from(b'x')));
    assert_eq!(putc(file, i32::from(b'y')), Ok(i32::from(b'y')));
    assert!(state.env().contents(b"/f").is_empty());
    crate::flush::fflush(state, Some(file)).expect("fflush");
    assert_eq!(state.env().contents(b"/f"), b"xy".to_vec());
}

#[test]
fn unlocked_forms_do_not_lock() {
    let state = state_with(b"z");
    let file = open_file(state, b"/f", b"r");
    assert_eq!(getc_unlocked(file), Ok(i32::from(b'z')));
    assert_eq!(fgetc_unlocked(file), Err(Fail::Eof));
    let out_state = leaked_state();
    out_state.env().add_file(b"/o", b"", 1024);
    let out = open_file(out_state, b"/o", b"w");
    assert_eq!(putc_unlocked(out, i32::from(b'q')), Ok(i32::from(b'q')));
    assert_eq!(fputc_unlocked(out, i32::from(b'r')), Ok(i32::from(b'r')));
}

#[test]
fn unbuffered_putc_reaches_the_fd_immediately() {
    let state = leaked_state();
    state.env().add_file(b"/f", b"", 1024);
    let file = open_file(state, b"/f", b"w");
    crate::setvbuf::setvbuf(file, None, crate::setvbuf::IONBF, 0).expect("setvbuf");
    state.env().clear_trace();
    assert_eq!(putc(file, i32::from(b'!')), Ok(i32::from(b'!')));
    let trace = state.env().trace();
    assert_eq!(trace.len(), 1);
    assert!(matches!(
        trace[0],
        crate::fake_ut::Op::Write2 { len: 1, ret: 1, .. }
    ));
}

#[test]
fn wide_stream_rejects_byte_io() {
    let state = state_with(b"x");
    let file = open_file(state, b"/f", b"r");
    file.st_mut().mode = 1;
    assert_eq!(
        getc(file),
        Err(Fail::from_int(raven_libc_abi::generated::errno::EBADF))
    );
    assert_ne!(file.st().flags & F_ERR, 0);
}

#[test]
fn write_only_stream_reading_is_ebadf() {
    let state = leaked_state();
    state.env().add_file(b"/f", b"", 1024);
    let file = open_file(state, b"/f", b"w");
    assert_eq!(
        fgetc(file),
        Err(Fail::from_int(raven_libc_abi::generated::errno::EBADF))
    );
    assert_ne!(file.st().flags & F_ERR, 0);
}

#[test]
fn interrupted_read_is_not_retried() {
    let state = state_with(b"abc");
    let file = open_file(state, b"/f", b"r");
    state.env().eintr_on(1);
    assert!(getc(file).is_err());
    assert_ne!(file.st().flags & F_ERR, 0);
    let reads = state
        .env()
        .trace()
        .iter()
        .filter(|op| matches!(op, crate::fake_ut::Op::Read { .. }))
        .count();
    assert_eq!(reads, 1);
    assert_eq!(getc(file), Ok(i32::from(b'a')));
}

#[test]
fn write_error_is_sticky_until_clearerr() {
    let state = leaked_state();
    state.env().add_file(b"/f", b"", 1024);
    let file = open_file(state, b"/f", b"w");
    crate::setvbuf::setvbuf(file, None, crate::setvbuf::IONBF, 0).expect("setvbuf");
    state.env().fail_write_after(1);
    assert_eq!(
        putc(file, i32::from(b'a')),
        Err(Fail::from_int(raven_libc_abi::generated::errno::ENOSPC))
    );
    assert_ne!(file.st().flags & F_ERR, 0);
    crate::status::clearerr(file);
    assert_eq!(file.st().flags & F_ERR, 0);
    assert_eq!(putc(file, i32::from(b'b')), Ok(i32::from(b'b')));
}

#[test]
fn getchar_and_putchar_use_the_standard_slots() {
    let state = leaked_state();
    let streams = leaked_streams(state);
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
        .data = b"k".to_vec();
    assert_eq!(getchar(state), Ok(i32::from(b'k')));
    assert_eq!(putchar(state, i32::from(b'\n')), Ok(i32::from(b'\n')));
    assert_eq!(
        putchar_unlocked(state, i32::from(b'!')),
        Ok(i32::from(b'!'))
    );
    assert_eq!(getchar_unlocked(state), Err(Fail::Eof));
}

#[test]
fn uflow_reads_one_byte_after_the_buffer_is_empty() {
    let state = state_with(b"0123456789");
    let file = open_file(state, b"/f", b"r");
    assert_eq!(uflow(file), Ok(i32::from(b'0')));
    assert_eq!(fgetc(file), Ok(i32::from(b'1')));
}
