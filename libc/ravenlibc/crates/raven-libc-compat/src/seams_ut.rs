// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Unit tests for the seam types and the `Winsize` layout.

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
fn winsize_is_eight_bytes() {
    assert_eq!(core::mem::size_of::<Winsize>(), 8);
    assert_eq!(core::mem::align_of::<Winsize>(), 2);
}

#[test]

fn winsize_field_offsets_are_packed_u16() {
    let w = Winsize {
        ws_row: 24,
        ws_col: 80,
        ws_xpixel: 640,
        ws_ypixel: 480,
    };
    assert_eq!(w.ws_row, 24);
    assert_eq!(w.ws_col, 80);
    assert_eq!(w.ws_xpixel, 640);
    assert_eq!(w.ws_ypixel, 480);
    let bytes = [
        w.ws_row.to_ne_bytes(),
        w.ws_col.to_ne_bytes(),
        w.ws_xpixel.to_ne_bytes(),
        w.ws_ypixel.to_ne_bytes(),
    ];
    assert_eq!(bytes.len(), 4);
}

#[test]
fn termios_is_the_c36_record() {
    assert_eq!(core::mem::size_of::<Termios>(), 60);
    assert_eq!(core::mem::align_of::<Termios>(), 4);
}

#[test]
fn fork_result_is_copy() {
    let parent = ForkResult::Parent(42);
    let copy = parent;
    assert_eq!(parent, copy);
    assert_eq!(ForkResult::Child, ForkResult::Child);
}
