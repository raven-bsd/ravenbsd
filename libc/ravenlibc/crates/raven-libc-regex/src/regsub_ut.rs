// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Unit tests of `regnsub` and `regasub`.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects
)]

use super::*;

fn matches() -> [RegMatch; 4] {
    [
        RegMatch { rm_so: 0, rm_eo: 3 },
        RegMatch { rm_so: 1, rm_eo: 2 },
        RegMatch {
            rm_so: -1,
            rm_eo: -1,
        },
        RegMatch { rm_so: 0, rm_eo: 0 },
    ]
}

#[test]
fn whole_match_with_ampersand() {
    let mut buffer = [0u8; 16];
    let length = regnsub(&mut buffer, b"<&>", &matches(), b"abc");
    assert_eq!(length, 5);
    assert_eq!(&buffer[..5], b"<abc>");
}

#[test]
fn numbered_groups() {
    let mut buffer = [0u8; 16];
    let length = regnsub(&mut buffer, b"\\1-\\0", &matches(), b"abc");
    assert_eq!(length, 5);
    assert_eq!(&buffer[..5], b"b-abc");
}

#[test]
fn unmatched_group_is_skipped() {
    let mut buffer = [0u8; 16];
    let length = regnsub(&mut buffer, b"[\\2]", &matches(), b"abc");
    assert_eq!(length, 2);
    assert_eq!(&buffer[..2], b"[]");
}

#[test]
fn escaped_backslash_and_ampersand_are_literal() {
    let mut buffer = [0u8; 16];
    let length = regnsub(&mut buffer, b"\\\\&\\&", &matches(), b"abc");
    assert_eq!(length, 5);
    assert_eq!(&buffer[..5], b"\\abc&");
}

#[test]
fn a_trailing_backslash_is_copied() {
    let mut buffer = [0u8; 16];
    let length = regnsub(&mut buffer, b"x\\", &matches(), b"abc");
    assert_eq!(length, 2);
    assert_eq!(&buffer[..2], b"x\\");
}

#[test]
fn empty_group_four_is_an_empty_match() {
    let mut buffer = [0u8; 16];
    let length = regnsub(&mut buffer, b"\\3<\\4>", &matches(), b"abc");
    assert_eq!(length, 2);
    assert_eq!(&buffer[..2], b"<>");
}

#[test]
fn fixed_buffer_exactly_full_truncates_with_a_nul_at_the_last_byte() {
    let mut buffer = [b'.'; 3];
    let length = regnsub(&mut buffer, b"abc", &matches(), b"abc");
    assert_eq!(length, 3);
    assert_eq!(&buffer[..2], b"ab");
    assert_eq!(buffer[2], 0);
}

#[test]
fn fixed_buffer_one_short_counts_every_byte() {
    let mut buffer = [b'.'; 2];
    let length = regnsub(&mut buffer, b"abcdef", &matches(), b"abc");
    assert_eq!(length, 6);
    assert_eq!(buffer[1], 0);
}

#[test]
fn allocating_variant_returns_the_text_and_grows() {
    let mut long = vec![b'x'; 200];
    long.extend_from_slice(b"!");
    let result = regasub(b"<&>", &matches(), &long).expect("space");
    assert_eq!(result.len(), 5);
    assert_eq!(&result[..], b"<xxx>");
}

#[test]
fn a_zero_length_fixed_buffer_only_counts() {
    let mut buffer = [0u8; 0];
    let length = regnsub(&mut buffer, b"abc", &matches(), b"abc");
    assert_eq!(length, 3);
}

/// Ten groups `[i, i+1)` over `0123456789`.
fn ten() -> [RegMatch; 10] {
    let mut table = [RegMatch {
        rm_so: -1,
        rm_eo: -1,
    }; 10];
    for (index, slot) in table.iter_mut().enumerate() {
        slot.rm_so = index as i64;
        slot.rm_eo = index as i64 + 1;
    }
    table
}

macro_rules! group_cases {
    ($($name:ident => ($index:expr, $byte:expr);)*) => {
        $(
            #[test]
            fn $name() {
                let sub = [b'\\', b'0' + $index];
                let mut buffer = [0u8; 8];
                let length = regnsub(&mut buffer, &sub, &ten(), b"0123456789");
                assert_eq!(length, 1);
                assert_eq!(buffer[0], $byte);
            }
        )*
    };
}

group_cases! {
    group_zero => (0, b'0');
    group_one => (1, b'1');
    group_two => (2, b'2');
    group_three => (3, b'3');
    group_four => (4, b'4');
    group_five => (5, b'5');
    group_six => (6, b'6');
    group_seven => (7, b'7');
    group_eight => (8, b'8');
    group_nine => (9, b'9');
}

#[test]
fn unknown_group_is_silently_skipped() {
    let table = ten();
    let mut buffer = [0u8; 8];
    let length = regnsub(&mut buffer, b"x\\:", &table, b"0123456789");
    // `\:` is not a special escape: the backslash is copied, then ':'.
    assert_eq!(length, 3);
    assert_eq!(&buffer[..3], b"x\\:");
}

#[test]
fn amp_with_unmatched_zero_drops_the_group() {
    let mut table = ten();
    table[0].rm_so = -1;
    table[0].rm_eo = -1;
    let mut buffer = [0u8; 8];
    let length = regnsub(&mut buffer, b"<&>", &table, b"0123456789");
    assert_eq!(length, 2);
    assert_eq!(&buffer[..2], b"<>");
}

#[test]
fn nul_in_the_substitution_stops_it() {
    let mut buffer = [0u8; 8];
    let length = regnsub(&mut buffer, b"ab\0cd", &ten(), b"0123456789");
    assert_eq!(length, 2);
    assert_eq!(&buffer[..2], b"ab");
}

#[test]
fn source_bytes_are_copied_including_nul() {
    let source = [b'a', 0, b'b'];
    let table = [RegMatch { rm_so: 0, rm_eo: 3 }; 1];
    let mut buffer = [0u8; 8];
    let length = regnsub(&mut buffer, b"\\0", &table, &source);
    assert_eq!(length, 3);
    assert_eq!(&buffer[..3], &source);
}

#[test]
fn allocating_variant_empty_substitution() {
    let result = regasub(b"", &ten(), b"0123456789").expect("space");
    assert!(result.is_empty());
}
