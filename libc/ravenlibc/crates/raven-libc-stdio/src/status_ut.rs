// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! Unit tests for [`crate::status`].
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

#[test]
fn clearerr_clears_both_indicators() {
    let state = leaked_state();
    state.env().add_file(b"/f", b"", 1024);
    let file = open_file(state, b"/f", b"r");
    file.st_mut().flags |= crate::file::F_EOF | crate::file::F_ERR;
    assert!(feof(file));
    assert!(ferror(file));
    clearerr(file);
    assert!(!feof(file));
    assert!(!ferror(file));
}

#[test]
fn feof_and_ferror_read_the_flags() {
    let state = leaked_state();
    state.env().add_file(b"/f", b"", 1024);
    let file = open_file(state, b"/f", b"r");
    assert!(!feof(file));
    assert!(!ferror(file));
    file.st_mut().flags |= crate::file::F_EOF;
    assert!(feof(file));
    assert!(!ferror(file));
    file.st_mut().flags |= crate::file::F_ERR;
    assert!(ferror(file));
}

#[test]
fn fileno_returns_the_descriptor_or_ebadf() {
    let state = leaked_state();
    state.env().add_file(b"/f", b"", 1024);
    let file = open_file(state, b"/f", b"r");
    assert_eq!(fileno(file).map_err(|e| e.errno().map(|e| e.0)), Ok(3));
    file.st_mut().fd = -1;
    assert_eq!(
        fileno(file).map_err(|e| e.errno().map(|e| e.0)),
        Err(Some(raven_libc_abi::generated::errno::EBADF as u16))
    );
}
