// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! Unit tests for [`crate::exit`]: the cleanup order, the forever lock, reader repositioning
//! and the fork hooks (T11).
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
use crate::fake_ut::{Op, leaked_state, leaked_streams, open_file};
use std::sync::mpsc;
use std::time::Duration;

#[test]
fn cleanup_locks_files_forever_and_repositions_readers() {
    let state = leaked_state();
    state.env().install_std_file();
    state.env().add_file(b"/r", b"abcdef", 1024);
    let streams = leaked_streams(state);
    let reader = open_file(state, b"/r", b"r");
    assert_eq!(crate::char_io::getc(reader), Ok(i32::from(b'a')));
    state.env().clear_trace();
    stdio_cleanup(state, streams);
    let lock = reader.st().lock.load(core::sync::atomic::Ordering::Relaxed);
    assert_ne!(lock, 0, "cleanup must leave the FILE locked");
    let trace = state.env().trace();
    let seek = trace.iter().find_map(|op| match op {
        Op::Seek { off, whence, .. } => Some((*off, *whence)),
        _ => None,
    });
    assert!(
        seek.is_some(),
        "read stream was not repositioned: {trace:?}"
    );
}

#[test]
fn cleanup_does_not_close_or_free() {
    let state = leaked_state();
    state.env().install_std_file();
    state.env().add_file(b"/w", b"", 1024);
    let streams = leaked_streams(state);
    let writer = open_file(state, b"/w", b"w");
    assert_eq!(crate::block_io::fwrite(writer, b"x", 1, 1), Ok(1));
    stdio_cleanup(state, streams);
    assert_eq!(state.env().contents(b"/w"), b"x".to_vec());
    assert_eq!(
        state
            .env()
            .trace()
            .iter()
            .filter(|op| matches!(op, Op::Close { .. }))
            .count(),
        0
    );
    assert!(state.ofl_head().is_some());
}

#[test]
fn second_writer_blocks_after_cleanup() {
    let state = leaked_state();
    state.env().install_std_file();
    state.env().add_file(b"/w", b"", 1024);
    let streams = leaked_streams(state);
    let writer = open_file(state, b"/w", b"w");
    stdio_cleanup(state, streams);
    let (sender, receiver) = mpsc::channel();
    std::thread::spawn(move || {
        crate::lock::flockfile(writer);
        let _ = sender.send(());
    });
    assert!(
        receiver.recv_timeout(Duration::from_millis(200)).is_err(),
        "the forever lock must block a second writer"
    );
}

#[test]
fn fork_hooks_only_touch_the_list_lock() {
    let state = leaked_state();
    let file = crate::fake_ut::leaked_file(state);
    crate::lock::flockfile(file);
    fork_prepare(state);
    assert_ne!(
        state
            .ofl_lock_word()
            .load(core::sync::atomic::Ordering::Relaxed),
        0
    );
    let held = file.st().lock.load(core::sync::atomic::Ordering::Relaxed);
    fork_child(state);
    assert_eq!(
        state
            .ofl_lock_word()
            .load(core::sync::atomic::Ordering::Relaxed),
        0
    );
    assert_eq!(
        file.st().lock.load(core::sync::atomic::Ordering::Relaxed),
        held,
        "FILE locks are not reset by the stdio fork hooks"
    );
    crate::lock::funlockfile(file);
}
