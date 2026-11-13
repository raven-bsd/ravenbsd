// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#![allow(
    clippy::arithmetic_side_effects,
    clippy::indexing_slicing,
    clippy::panic,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::assertions_on_constants
)]

//! `assert_message`: byte-exact H-13 text, the three entry points and truncation.

use super::*;

fn message(prog: &[u8], file: &[u8], line: i32, func: &[u8], expr: &[u8]) -> Vec<u8> {
    let mut out = [0u8; MESSAGE_CAPACITY];
    let count = assert_message(&mut out, prog, file, line, func, expr);
    out[..count].to_vec()
}

#[test]
fn glibc_text() {
    assert_eq!(
        message(b"prog", b"file.c", 42, b"main", b"x > 0"),
        b"prog: file.c:42: main: Assertion `x > 0' failed.\n"
    );
}

#[test]
fn empty_prog_and_func_print_question_mark() {
    assert_eq!(
        message(b"", b"f.c", 1, b"", b"0"),
        b"?: f.c:1: ?: Assertion `0' failed.\n"
    );
}

#[test]
fn negative_line_and_empty_file() {
    assert_eq!(
        message(b"p", b"", -7, b"f", b"e"),
        b"p: :-7: f: Assertion `e' failed.\n"
    );
}

#[test]
fn the_three_entry_points_agree() {
    let mut a = [0u8; MESSAGE_CAPACITY];
    let mut b = [0u8; MESSAGE_CAPACITY];
    let mut c = [0u8; MESSAGE_CAPACITY];
    let na = assert_fail(&mut a, b"p", b"x", b"f.c", 3, b"fn");
    let nb = assert2(&mut b, b"p", b"f.c", 3, b"fn", b"x");
    let nc = assert_bsd(&mut c, b"p", b"f.c", 3, b"x");
    assert_eq!(&a[..na], &b[..nb]);
    assert_eq!(&c[..nc], b"p: f.c:3: ?: Assertion `x' failed.\n");
}

#[test]
fn truncates_at_511_bytes_and_keeps_the_newline_when_there_is_room() {
    let expr = [b'x'; 600];
    let mut out = [0u8; MESSAGE_CAPACITY];
    let count = assert_message(&mut out, b"p", b"f", 1, b"fn", &expr);
    assert_eq!(count, MESSAGE_CAPACITY - 1);
    assert_eq!(out[count - 1], b'\n');
    assert_eq!(&out[..4], b"p: f");
}

#[test]
fn exactly_the_capacity_still_ends_with_newline() {
    let prefix = b"p: f:1: fn: Assertion `";
    let suffix = b"' failed.";
    let expr_len = MESSAGE_CAPACITY - 2 - prefix.len() - suffix.len();
    let expr = vec![b'x'; expr_len];
    let mut out = [0u8; MESSAGE_CAPACITY];
    let count = assert_message(&mut out, b"p", b"f", 1, b"fn", &expr);
    assert_eq!(count, MESSAGE_CAPACITY - 1);
    assert_eq!(out[count - 1], b'\n');
}

#[test]
fn empty_expr() {
    assert_eq!(
        message(b"p", b"f", 1, b"fn", b""),
        b"p: f:1: fn: Assertion `' failed.\n"
    );
}

#[test]
fn long_file_is_truncated_not_panicking() {
    let file = [b'f'; 1000];
    let mut out = [0u8; MESSAGE_CAPACITY];
    let count = assert_message(&mut out, b"p", &file, 1, b"fn", b"x");
    assert_eq!(count, MESSAGE_CAPACITY - 1);
    assert_eq!(out[count - 1], b'\n');
}

#[test]
fn assert_fail_and_assert2_are_identical() {
    let mut a = [0u8; MESSAGE_CAPACITY];
    let mut b = [0u8; MESSAGE_CAPACITY];
    let na = assert_fail(&mut a, b"p", b"expr", b"file", 9, b"fun");
    let nb = assert2(&mut b, b"p", b"file", 9, b"fun", b"expr");
    assert_eq!(&a[..na], &b[..nb]);
}

#[test]
fn bsd_entry_uses_question_mark() {
    let mut out = [0u8; MESSAGE_CAPACITY];
    let count = assert_bsd(&mut out, b"p", b"f", 2, b"e");
    assert_eq!(&out[..count], b"p: f:2: ?: Assertion `e' failed.\n");
}
