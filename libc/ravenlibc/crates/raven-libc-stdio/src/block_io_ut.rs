// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! Unit tests for [`crate::block_io`]: items vs bytes, the direct-read bypass, short writes
//! and the overflow checks.
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

fn read_state(data: &[u8]) -> &'static crate::state::State<FakeEnv> {
    let state = leaked_state();
    state.env().add_file(b"/f", data, 1024);
    state
}

#[test]
fn fread_returns_items_and_exhausts_the_file() {
    let state = read_state(b"abcdef");
    let file = open_file(state, b"/f", b"r");
    let mut buffer = [0u8; 4];
    assert_eq!(fread(file, &mut buffer, 2, 2), Ok(2));
    assert_eq!(&buffer, b"abcd");
    let mut tail = [0u8; 4];
    assert_eq!(fread(file, &mut tail, 1, 4), Ok(2));
    assert_eq!(&tail[..2], b"ef");
}

#[test]
fn fread_large_request_bypasses_the_buffer() {
    let data: std::vec::Vec<u8> = (0..5000u32).map(|i| (i % 251) as u8).collect();
    let state = read_state(&data);
    let file = open_file(state, b"/f", b"r");
    let mut buffer = std::vec![0u8; 5000];
    assert_eq!(fread(file, &mut buffer, 1, 5000), Ok(5000));
    assert_eq!(buffer, data);
    let reads = state
        .env()
        .trace()
        .iter()
        .filter(|op| matches!(op, crate::fake_ut::Op::Read { .. }))
        .count();
    assert!(
        reads <= 2,
        "direct read should not fill the FILE buffer repeatedly: {reads}"
    );
}

#[test]
fn fread_with_zero_size_returns_zero() {
    let state = read_state(b"abc");
    let file = open_file(state, b"/f", b"r");
    let mut buffer = [0u8; 8];
    assert_eq!(fread(file, &mut buffer, 0, 8), Ok(0));
    assert_eq!(fread(file, &mut buffer, 8, 0), Ok(0));
    assert_eq!(buffer, [0u8; 8]);
}

#[test]
fn fread_overflow_is_eoverflow() {
    let state = read_state(b"abc");
    let file = open_file(state, b"/f", b"r");
    let mut buffer = [0u8; 8];
    assert_eq!(
        fread(file, &mut buffer, usize::MAX, 2),
        Err(Fail::from_int(raven_libc_abi::generated::errno::EOVERFLOW))
    );
}

#[test]
fn fwrite_returns_items_and_short_writes_stay_short() {
    let state = leaked_state();
    state.env().add_file(b"/f", b"", 1024);
    let file = open_file(state, b"/f", b"w");
    assert_eq!(fwrite(file, b"abcdef", 2, 3), Ok(3));
    crate::flush::fflush(state, Some(file)).expect("fflush");
    assert_eq!(state.env().contents(b"/f"), b"abcdef".to_vec());
}

#[test]
fn fwrite_zero_size_does_not_touch_the_stream() {
    let state = leaked_state();
    state.env().add_file(b"/f", b"", 1024);
    let file = open_file(state, b"/f", b"w");
    assert_eq!(fwrite(file, b"abc", 0, 3), Ok(0));
    assert_eq!(fwrite(file, b"abc", 3, 0), Ok(0));
    assert!(!file.st().has_pending_write());
}

#[test]
fn fwrite_loops_on_a_short_gathering_write() {
    let state = leaked_state();
    state.env().add_file(b"/f", b"", 1024);
    let file = open_file(state, b"/f", b"w");
    state.env().set_max_write(3);
    let data = *b"0123456789";
    assert_eq!(fwrite(file, &data, 1, 10), Ok(10));
    crate::flush::fflush(state, Some(file)).expect("fflush");
    assert_eq!(state.env().contents(b"/f"), data.to_vec());
    let writes = state
        .env()
        .trace()
        .iter()
        .filter(|op| {
            matches!(
                op,
                crate::fake_ut::Op::Write2 { .. } | crate::fake_ut::Op::Write { .. }
            )
        })
        .count();
    assert!(writes >= 3);
}

#[test]
fn fwrite_unbuffered_is_one_write() {
    let state = leaked_state();
    state.env().add_file(b"/f", b"", 1024);
    let file = open_file(state, b"/f", b"w");
    crate::setvbuf::setvbuf(file, None, crate::setvbuf::IONBF, 0).expect("setvbuf");
    state.env().clear_trace();
    assert_eq!(fwrite(file, b"hello", 1, 5), Ok(5));
    let trace = state.env().trace();
    assert_eq!(trace.len(), 1);
    assert!(matches!(
        trace[0],
        crate::fake_ut::Op::Write2 { len: 5, ret: 5, .. }
    ));
}
