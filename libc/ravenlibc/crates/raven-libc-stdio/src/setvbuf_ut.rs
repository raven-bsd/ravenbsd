// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! Unit tests for [`crate::setvbuf`].
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
use crate::fake_ut::{leaked_state, open_file};
use crate::file::{F_LAZYBUF, F_SVB, UNGET};

#[test]
fn ionbf_clears_the_buffer_size() {
    let state = leaked_state();
    state.env().add_file(b"/f", b"", 1024);
    let file = open_file(state, b"/f", b"w");
    assert_eq!(setvbuf(file, None, IONBF, 0), Ok(()));
    assert_eq!(file.st().buf_size, 0);
    assert_ne!(file.st().flags & F_SVB, 0);
    assert_eq!(file.st().flags & F_LAZYBUF, 0);
    assert_eq!(file.st().lbf, -1);
}

#[test]
fn iolbf_sets_the_newline_byte() {
    let state = leaked_state();
    state.env().add_file(b"/f", b"", 1024);
    let file = open_file(state, b"/f", b"w");
    assert_eq!(setvbuf(file, None, IOLBF, 0), Ok(()));
    assert_eq!(file.st().lbf, i32::from(b'\n'));
}

#[test]
fn a_caller_buffer_is_used_from_the_unget_offset() {
    let state = leaked_state();
    state.env().add_file(b"/f", b"", 1024);
    let file = open_file(state, b"/f", b"w");
    let mut buffer = std::vec![0u8; 100];
    assert_eq!(
        setvbuf(file, Some((buffer.as_mut_ptr(), buffer.len())), IOFBF, 100),
        Ok(())
    );
    assert_eq!(file.st().buf_size, 100 - UNGET);
    assert_eq!(file.st().buffer.bytes().len(), 100);
}

#[test]
fn a_too_small_buffer_is_ignored() {
    let state = leaked_state();
    state.env().add_file(b"/f", b"", 1024);
    let file = open_file(state, b"/f", b"w");
    let mut buffer = std::vec![0u8; 4];
    assert_eq!(
        setvbuf(file, Some((buffer.as_mut_ptr(), buffer.len())), IOFBF, 4),
        Ok(())
    );
    assert!(file.st().buf_size > 0);
    assert_ne!(file.st().flags & F_SVB, 0);
}

#[test]
fn a_bad_type_is_einval_and_keeps_the_stream() {
    let state = leaked_state();
    state.env().add_file(b"/f", b"", 1024);
    let file = open_file(state, b"/f", b"w");
    assert_eq!(
        setvbuf(file, None, 42, 0),
        Err(Fail::from_int(raven_libc_abi::generated::errno::EINVAL))
    );
    assert_eq!(file.st().flags & F_SVB, 0);
}

#[test]
fn bsd_wrappers() {
    let state = leaked_state();
    state.env().add_file(b"/f", b"", 1024);
    let file = open_file(state, b"/f", b"w");
    let mut buffer = std::vec![0u8; 64];
    setbuf(file, Some((buffer.as_mut_ptr(), buffer.len())));
    assert_eq!(file.st().buf_size, crate::buf::BUFSIZ.min(64) - UNGET);
    setlinebuf(file);
    assert_eq!(file.st().lbf, i32::from(b'\n'));
    setbuffer(file, None, 0);
    assert_eq!(file.st().buf_size, 0);
}
