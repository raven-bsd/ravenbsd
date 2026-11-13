// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! Unit tests for [`crate::std_streams`]: the three FILEs and their `.bss` buffers.
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
use crate::fake_ut::{leaked_state, leaked_streams};
use crate::file::UNGET;

#[test]
fn standard_streams_have_the_documented_initial_state() {
    let state = leaked_state();
    let streams = leaked_streams(state);
    let stdin = streams.stdin();
    let stdout = streams.stdout();
    let stderr = streams.stderr();
    assert_eq!(stdin.fd(), 0);
    assert_eq!(stdout.fd(), 1);
    assert_eq!(stderr.fd(), 2);
    assert!(stdin.is_perm());
    assert!(stdout.is_perm());
    assert!(stderr.is_perm());
    assert_eq!(stdin.st().buf_size, STD_BUF);
    assert_eq!(stdout.st().buf_size, STD_BUF);
    assert_eq!(stderr.st().buf_size, 0);
    assert_eq!(stdout.st().lbf, i32::from(b'\n'));
    assert!(stdout.st().probe);
    assert!(!stdin.st().probe);
    assert_eq!(stdin.st().buffer.bytes().len(), UNGET + STD_BUF);
    assert_eq!(stderr.st().buffer.bytes().len(), UNGET);
    assert_eq!(stderr.st().rpos, UNGET);
    assert_eq!(stderr.st().rend, UNGET);
}

#[test]
fn accessors_register_the_slots() {
    let state = leaked_state();
    let streams = leaked_streams(state);
    let stdout = streams.stdout();
    assert!(
        state
            .std_file(crate::state::StdKind::Out)
            .is_some_and(|f| core::ptr::eq(f, stdout))
    );
    let stdin = streams.stdin();
    assert!(
        state
            .std_file(crate::state::StdKind::In)
            .is_some_and(|f| core::ptr::eq(f, stdin))
    );
    let all = streams.all();
    assert_eq!(all.len(), 3);
    assert_eq!(all[2].fd(), 2);
}

#[test]
fn repeated_access_is_idempotent() {
    let state = leaked_state();
    let streams = leaked_streams(state);
    let first = streams.stdin();
    let second = streams.stdin();
    assert!(core::ptr::eq(first, second));
    assert_eq!(first.st().buffer.bytes().len(), UNGET + STD_BUF);
}
