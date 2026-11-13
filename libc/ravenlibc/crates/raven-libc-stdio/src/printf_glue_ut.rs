// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! Unit tests for [`crate::printf_glue`]: the dry pass, the unbuffered single write, the
//! error-flag save/restore and `vdprintf`/`vasprintf`.
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
use crate::fake_ut::{EngineArgs, Op, leaked_state, open_file};
use raven_libc_fmt::{FmtOptions, NoFloat};

#[test]
fn vfprintf_writes_through_the_engine() {
    let state = leaked_state();
    state.env().add_file(b"/f", b"", 1024);
    let file = open_file(state, b"/f", b"w");
    let mut args = EngineArgs::default();
    let ptr = args.cstring(b"world");
    args.words.push(42);
    args.words.push(ptr as u64);
    let result = vfprintf_file(
        file,
        b"hi %d %s",
        &mut args,
        &FmtOptions::default(),
        &NoFloat,
    );
    assert!(result == Ok(11));
    crate::flush::fflush(state, Some(file)).expect("fflush");
    assert_eq!(state.env().contents(b"/f"), b"hi 42 world".to_vec());
}

#[test]
fn vfprintf_to_an_unbuffered_stream_is_one_write() {
    let state = leaked_state();
    state.env().add_file(b"/f", b"", 1024);
    let file = open_file(state, b"/f", b"w");
    crate::setvbuf::setvbuf(file, None, crate::setvbuf::IONBF, 0).expect("setvbuf");
    state.env().clear_trace();
    let mut args = EngineArgs::default();
    args.words.push(7);
    let result = vfprintf_file(
        file,
        b"value=%d",
        &mut args,
        &FmtOptions::default(),
        &NoFloat,
    );
    assert!(result == Ok(7));
    let trace = state.env().trace();
    assert_eq!(
        trace.len(),
        1,
        "unbuffered printf must be one write: {trace:?}"
    );
    assert!(matches!(trace[0], Op::Write2 { len: 7, ret: 7, .. }));
}

#[test]
fn vfprintf_restores_the_old_error_flag() {
    let state = leaked_state();
    state.env().add_file(b"/f", b"", 1024);
    let file = open_file(state, b"/f", b"w");
    file.st_mut().flags |= crate::file::F_ERR;
    let mut args = EngineArgs::default();
    let result = vfprintf_file(file, b"x", &mut args, &FmtOptions::default(), &NoFloat);
    assert!(result == Ok(1));
    assert_ne!(file.st().flags & crate::file::F_ERR, 0);
}

#[test]
fn a_positional_gap_is_refused_before_writing() {
    let state = leaked_state();
    state.env().add_file(b"/f", b"", 1024);
    let file = open_file(state, b"/f", b"w");
    let mut args = EngineArgs::default();
    args.words.push(1);
    let result = vfprintf_file(file, b"%2$d", &mut args, &FmtOptions::default(), &NoFloat);
    assert!(result == Err(raven_libc_fmt::FmtErr::Invalid));
    crate::flush::fflush(state, Some(file)).expect("fflush");
    assert!(state.env().contents(b"/f").is_empty());
}

#[test]
fn vdprintf_writes_to_a_descriptor() {
    let state = leaked_state();
    let fd = state
        .env()
        .push_fd(b"", crate::fake_ut::Kind::Regular(1024));
    let mut args = EngineArgs::default();
    let ptr = args.cstring(b"abc");
    args.words.push(ptr as u64);
    let result = vdprintf_fd(
        state.env(),
        fd,
        b"[%s]",
        &mut args,
        &FmtOptions::default(),
        &NoFloat,
    );
    assert!(result == Ok(5));
    assert_eq!(state.env().data(fd), b"[abc]".to_vec());
}

#[test]
fn vasprintf_counts_allocates_and_formats_again() {
    let mut args = EngineArgs::default();
    let ptr = args.cstring(b"ok");
    args.words.push(ptr as u64);
    let buffer = vasprintf_core(b"<%s>", &mut args, &FmtOptions::default(), &NoFloat)
        .ok()
        .expect("vasprintf");
    assert_eq!(buffer, b"<ok>\0".to_vec());
}

#[test]
fn vfwprintf_writes_wide_text() {
    let state = leaked_state();
    state.env().add_file(b"/f", b"", 1024);
    let file = open_file(state, b"/f", b"w");
    let mut args = EngineArgs::default();
    let fmt: Vec<u32> = "hi €".chars().map(|c| c as u32).collect();
    let result = vfwprintf_file(file, &fmt, &mut args, &FmtOptions::default(), &NoFloat);
    assert!(result == Ok(4));
    crate::flush::fflush(state, Some(file)).expect("fflush");
    assert_eq!(state.env().contents(b"/f"), "hi €".as_bytes().to_vec());
}
