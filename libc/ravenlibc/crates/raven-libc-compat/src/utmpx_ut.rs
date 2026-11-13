// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `utmpx` stub tests.

#![allow(
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::byte_char_slices,
    clippy::field_reassign_with_default,
    clippy::unreachable,
    clippy::todo,
    clippy::unimplemented
)]

use super::*;

#[test]
fn enumerators_return_none() {
    setutxent();
    assert!(getutxent().is_none());
    assert!(getutxid(b"x").is_none());
    assert!(getutxline(b"x").is_none());
    endutxent();
}

#[test]
fn pututxline_returns_its_argument() {
    let line = b"ttyv0";
    assert_eq!(pututxline(line), line);
    assert!(pututxline_is_noop());
}

#[test]
fn updwtmpx_is_a_no_op() {
    updwtmpx(b"/var/run/utmpx", b"line");
}
