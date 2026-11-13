// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! Unit tests for [`crate::scanf_glue`].
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
use crate::fake_ut::{EngineArgs, leaked_state, open_file};
use raven_libc_fmt::{NoScanFloat, ScanOptions};

#[test]
fn vfscanf_stores_through_the_environment() {
    let state = leaked_state();
    state.env().add_file(b"/f", b"17 word", 1024);
    let file = open_file(state, b"/f", b"r");
    let mut args = EngineArgs::default();
    args.words.push(1);
    args.words.push(1);
    let result = vfscanf_file(
        file,
        b"%d %s",
        &mut args,
        &ScanOptions::default(),
        &NoScanFloat,
    );
    let result = result.ok().expect("vscan");
    assert_eq!(result.assigned, 2);
    assert!(!result.return_eof);
    assert_eq!(args.stored_int, std::vec![17u64]);
    assert_eq!(args.stored_chars, b"word\0".to_vec());
    assert_eq!(result.consumed, 7);
}

#[test]
fn vfscanf_returns_eof_when_nothing_matches() {
    let state = leaked_state();
    state.env().add_file(b"/f", b"", 1024);
    let file = open_file(state, b"/f", b"r");
    let mut args = EngineArgs::default();
    args.words.push(1);
    let result = vfscanf_file(
        file,
        b"%d",
        &mut args,
        &ScanOptions::default(),
        &NoScanFloat,
    )
    .ok()
    .expect("vscan");
    assert_eq!(result.assigned, 0);
    assert!(result.return_eof);
}

#[test]
fn vfscanf_leaves_the_next_byte_unread() {
    let state = leaked_state();
    state.env().add_file(b"/f", b"12x", 1024);
    let file = open_file(state, b"/f", b"r");
    let mut args = EngineArgs::default();
    args.words.push(1);
    let result = vfscanf_file(
        file,
        b"%2d",
        &mut args,
        &ScanOptions::default(),
        &NoScanFloat,
    )
    .ok()
    .expect("vscan");
    assert_eq!(result.assigned, 1);
    assert_eq!(args.stored_int, std::vec![12u64]);
    assert_eq!(crate::char_io::getc(file), Ok(i32::from(b'x')));
}

#[test]
fn vfscanf_refuses_a_wide_stream() {
    let state = leaked_state();
    state.env().add_file(b"/f", b"1", 1024);
    let file = open_file(state, b"/f", b"r");
    file.st_mut().mode = 1;
    let mut args = EngineArgs::default();
    args.words.push(1);
    assert!(
        vfscanf_file(
            file,
            b"%d",
            &mut args,
            &ScanOptions::default(),
            &NoScanFloat
        ) == Err(raven_libc_fmt::FmtErr::Invalid)
    );
}

#[test]
fn vfwscanf_reads_wide_input() {
    let state = leaked_state();
    state.env().add_file(b"/f", b"23", 1024);
    let file = open_file(state, b"/f", b"r");
    let fmt: Vec<u32> = "%d".chars().map(|c| c as u32).collect();
    let mut args = EngineArgs::default();
    args.words.push(1);
    let result = vfwscanf_file(file, &fmt, &mut args, &ScanOptions::default(), &NoScanFloat)
        .ok()
        .expect("vscan");
    assert_eq!(result.assigned, 1);
    assert_eq!(args.stored_int, std::vec![23u64]);
    assert_eq!(file.st().mode, 1);
}
