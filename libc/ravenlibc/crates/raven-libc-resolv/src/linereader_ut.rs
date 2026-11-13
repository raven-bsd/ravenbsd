// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Tests for the bounded line reader and the line helpers.

use super::*;

#[test]
fn reads_two_lines() {
    let mut reader = LineReader::new(b"alpha\nbeta\n", 16);
    assert_eq!(reader.next_line(), Some(&b"alpha"[..]));
    assert_eq!(reader.next_line(), Some(&b"beta"[..]));
    assert_eq!(reader.next_line(), None);
}

#[test]
fn strips_a_trailing_carriage_return() {
    let mut reader = LineReader::new(b"alpha\r\nbeta\r\n", 16);
    assert_eq!(reader.next_line(), Some(&b"alpha"[..]));
    assert_eq!(reader.next_line(), Some(&b"beta"[..]));
}

#[test]
fn accepts_a_last_line_without_a_newline() {
    let mut reader = LineReader::new(b"alpha\nbeta", 16);
    assert_eq!(reader.next_line(), Some(&b"alpha"[..]));
    assert_eq!(reader.next_line(), Some(&b"beta"[..]));
    assert_eq!(reader.next_line(), None);
}

#[test]
fn an_empty_input_has_no_lines() {
    let mut reader = LineReader::new(b"", 16);
    assert_eq!(reader.next_line(), None);
    assert_eq!(reader.skipped(), 0);
}

#[test]
fn an_empty_line_is_returned() {
    let mut reader = LineReader::new(b"\n", 16);
    assert_eq!(reader.next_line(), Some(&b""[..]));
    assert_eq!(reader.next_line(), None);
}

#[test]
fn a_line_of_exactly_max_is_accepted() {
    let mut reader = LineReader::new(b"12345678\n", 8);
    assert_eq!(reader.next_line(), Some(&b"12345678"[..]));
}

#[test]
fn a_line_one_over_max_is_skipped_whole() {
    let mut reader = LineReader::new(b"123456789\nnext\n", 8);
    assert_eq!(reader.next_line(), Some(&b"next"[..]));
    assert_eq!(reader.skipped(), 1);
}

#[test]
fn an_over_long_last_line_is_skipped() {
    let mut reader = LineReader::new(b"123456789", 8);
    assert_eq!(reader.next_line(), None);
    assert_eq!(reader.skipped(), 1);
}

#[test]
fn an_over_long_line_does_not_leak_its_tail() {
    let mut reader = LineReader::new(b"0123456789ABC\nshort\n", 8);
    assert_eq!(reader.next_line(), Some(&b"short"[..]));
    assert_eq!(reader.next_line(), None);
}

#[test]
fn skipped_counts_every_dropped_line() {
    let mut reader = LineReader::new(b"012345678\n012345678\nok\n", 8);
    assert_eq!(reader.next_line(), Some(&b"ok"[..]));
    assert_eq!(reader.skipped(), 2);
}

#[test]
fn position_advances_past_every_consumed_line() {
    let mut reader = LineReader::new(b"abc\ndef\n", 16);
    assert_eq!(reader.position(), 0);
    assert_eq!(reader.next_line(), Some(&b"abc"[..]));
    assert_eq!(reader.position(), 4);
    assert_eq!(reader.next_line(), Some(&b"def"[..]));
    assert_eq!(reader.position(), 8);
}

#[test]
fn rewind_restarts_the_reader() {
    let mut reader = LineReader::new(b"abc\ndef\n", 16);
    assert!(reader.next_line().is_some());
    assert!(reader.next_line().is_some());
    reader.rewind();
    assert_eq!(reader.next_line(), Some(&b"abc"[..]));
    assert_eq!(reader.skipped(), 0);
}

#[test]
fn trim_removes_both_ends() {
    assert_eq!(trim(b"  alpha\t\r "), b"alpha");
    assert_eq!(trim(b""), b"");
    assert_eq!(trim(b"   "), b"");
}

#[test]
fn words_drops_empty_tokens() {
    let tokens: alloc::vec::Vec<_> = words(b"  a\tb  c ").collect();
    assert_eq!(tokens, alloc::vec![&b"a"[..], &b"b"[..], &b"c"[..]]);
}

#[test]
fn strip_comment_cuts_at_hash() {
    assert_eq!(strip_comment(b"abc # def"), b"abc ");
    assert_eq!(strip_comment(b"abc"), b"abc");
    assert_eq!(strip_comment(b"# only"), b"");
}

#[test]
fn trim_cr_strips_only_one_carriage_return() {
    assert_eq!(trim_cr(b"abc\r"), b"abc");
    assert_eq!(trim_cr(b"abc\r\r"), b"abc\r");
    assert_eq!(trim_cr(b"abc"), b"abc");
}
