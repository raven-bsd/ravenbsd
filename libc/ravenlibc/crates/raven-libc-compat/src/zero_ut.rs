// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Secret scrubbing tests.

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
fn scrub_zeroes_bytes() {
    let mut buffer = [0xaa; 32];
    scrub(&mut buffer);
    assert_eq!(buffer, [0; 32]);
}

#[test]
fn scrub_zeroes_words() {
    let mut buffer = [0xdead_beef; 18];
    scrub_u32(&mut buffer);
    assert_eq!(buffer, [0; 18]);
}

#[test]
fn scrub_of_empty_is_a_no_op() {
    scrub(&mut []);
    scrub_u32(&mut []);
}
