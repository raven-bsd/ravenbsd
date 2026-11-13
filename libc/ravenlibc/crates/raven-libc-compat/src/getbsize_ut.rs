// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `getbsize` tests, transcribed from the donor's branches.

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

fn run(env: Option<&[u8]>) -> (Vec<u8>, i32, i64) {
    let mut headerlen = 0i32;
    let mut blocksize = 0i64;
    let header = getbsize(env, &mut headerlen, &mut blocksize);
    (header, headerlen, blocksize)
}

#[test]
fn default_is_512_blocks() {
    let (header, len, size) = run(None);
    assert_eq!(header, b"512-blocks");
    assert_eq!(len, 10);
    assert_eq!(size, 512);
}

#[test]
fn empty_value_is_the_default() {
    let (header, _, size) = run(Some(b""));
    assert_eq!(header, b"512-blocks");
    assert_eq!(size, 512);
}

#[test]
fn plain_number() {
    let (header, _, size) = run(Some(b"1024"));
    assert_eq!(header, b"1024-blocks");
    assert_eq!(size, 1024);
}

#[test]
fn zero_becomes_one() {
    // n=0 becomes 1, then the 1*1 underflow branch resets n and the size to 512.
    let (header, _, size) = run(Some(b"0"));
    assert_eq!(header, b"512-blocks");
    assert_eq!(size, 512);
}

#[test]
fn suffix_forms() {
    assert_eq!(run(Some(b"1K")), (b"1K-blocks".to_vec(), 9, 1024));
    assert_eq!(run(Some(b"1k")), (b"1K-blocks".to_vec(), 9, 1024));
    assert_eq!(
        run(Some(b"2M")),
        (b"2M-blocks".to_vec(), 9, 2 * 1024 * 1024)
    );
    assert_eq!(
        run(Some(b"1G")),
        (b"1G-blocks".to_vec(), 9, 1024 * 1024 * 1024)
    );
}

#[test]
fn maximum_is_one_gigabyte() {
    let (header, _, size) = run(Some(b"2G"));
    assert_eq!(header, b"1G-blocks");
    assert_eq!(size, 1024 * 1024 * 1024);
    let (header, _, size) = run(Some(b"2000000K"));
    assert_eq!(header, b"1048576K-blocks");
    assert_eq!(size, 1024 * 1024 * 1024);
}

#[test]
fn negative_and_malformed_fall_back_to_512() {
    for value in [b"-1".as_slice(), b"abc", b"1KB", b"1Q"] {
        let (header, _, size) = run(Some(value));
        assert_eq!(header, b"512-blocks", "value {value:?}");
        assert_eq!(size, 512, "value {value:?}");
    }
}

#[test]
fn whitespace_is_accepted_by_strtol() {
    let (header, _, size) = run(Some(b"  2K"));
    assert_eq!(header, b"2K-blocks");
    assert_eq!(size, 2048);
}

#[test]
fn header_length_is_capped_at_nineteen() {
    let (header, len, _) = run(Some(b"2000000K"));
    assert_eq!(header.len(), 15);
    assert_eq!(len, 15);
}

#[test]
fn minimum_blocksize_is_512() {
    let (header, _, size) = run(Some(b"1"));
    assert_eq!(header, b"512-blocks");
    assert_eq!(size, 512);
}
