// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `fparseln` tests: continuations, comments, escapes, line numbers and the 64 KiB cap.

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

use alloc::vec::Vec;

use super::*;
use crate::seams::LineSource;

struct Lines {
    lines: Vec<Vec<u8>>,
    cursor: usize,
}

impl Lines {
    fn new(text: &[u8]) -> Self {
        let mut lines = Vec::new();
        let mut start = 0usize;
        for (index, &byte) in text.iter().enumerate() {
            if byte == b'\n' {
                lines.push(text[start..=index].to_vec());
                start = index.wrapping_add(1);
            }
        }
        if start < text.len() {
            lines.push(text[start..].to_vec());
        }
        Self { lines, cursor: 0 }
    }
}

impl LineSource for Lines {
    fn next_line(&mut self) -> Option<Vec<u8>> {
        let line = self.lines.get(self.cursor).cloned();
        if line.is_some() {
            self.cursor = self.cursor.wrapping_add(1);
        }
        line
    }
}

fn parse(text: &[u8]) -> Vec<Vec<u8>> {
    let mut source = Lines::new(text);
    let mut out = Vec::new();
    while let Some(line) = fparseln(&mut source, [b'\\', b'\\', b'#'], 0, None) {
        out.push(line);
    }
    out
}

#[test]
fn plain_lines_drop_the_newline() {
    assert_eq!(
        parse(b"a\nb\nc\n"),
        alloc::vec![b"a".to_vec(), b"b".to_vec(), b"c".to_vec()]
    );
}

#[test]
fn blank_lines_are_returned_empty() {
    assert_eq!(
        parse(b"a\n\nb\n"),
        alloc::vec![b"a".to_vec(), Vec::new(), b"b".to_vec()]
    );
}

#[test]
fn final_line_without_newline() {
    assert_eq!(parse(b"a\nb"), alloc::vec![b"a".to_vec(), b"b".to_vec()]);
}

#[test]
fn empty_input_is_eof() {
    assert_eq!(parse(b""), Vec::<Vec<u8>>::new());
}

#[test]
fn comment_lines_are_skipped() {
    assert_eq!(parse(b"# one\n#two\na\n"), alloc::vec![b"a".to_vec()]);
}

#[test]
fn trailing_comment_is_removed() {
    assert_eq!(parse(b"a # comment\n"), alloc::vec![b"a ".to_vec()]);
}

#[test]
fn escaped_comment_character_is_kept() {
    assert_eq!(
        parse(b"a \\# not a comment\n"),
        alloc::vec![b"a \\# not a comment".to_vec()]
    );
}

#[test]
fn continuation_joins_physical_lines() {
    assert_eq!(parse(b"a\\\nb\n"), alloc::vec![b"ab".to_vec()]);
    assert_eq!(parse(b"a\\\n\\\nb\n"), alloc::vec![b"ab".to_vec()]);
}

#[test]
fn escaped_continuation_is_not_a_continuation() {
    assert_eq!(
        parse(b"a\\\\\nb\n"),
        alloc::vec![b"a\\\\".to_vec(), b"b".to_vec()]
    );
}

#[test]
fn line_numbers_count_physical_lines() {
    let mut source = Lines::new(b"# c\nx\\\ny\nz\n");
    let mut number = 0usize;
    let first = fparseln(&mut source, [b'\\', b'\\', b'#'], 0, Some(&mut number)).unwrap();
    assert_eq!(first, b"xy");
    assert_eq!(number, 3);
    let second = fparseln(&mut source, [b'\\', b'\\', b'#'], 0, Some(&mut number)).unwrap();
    assert_eq!(second, b"z");
    assert_eq!(number, 4);
    assert!(fparseln(&mut source, [b'\\', b'\\', b'#'], 0, Some(&mut number)).is_none());
    assert_eq!(number, 5);
}

#[test]
fn unesc_all_removes_the_selected_escapes() {
    let mut source = Lines::new(b"a\\#b\\nc\\\\d\\qe\n");
    let line = fparseln(&mut source, [b'\\', b'\\', b'#'], FPARSELN_UNESCALL, None).unwrap();
    assert_eq!(line, b"a#bnc\\dqe");
}

#[test]
fn unesc_rest_only_leaves_the_named_escapes() {
    let mut source = Lines::new(b"a\\#b\\nc\\\\d\\qe\n");
    let line = fparseln(&mut source, [b'\\', b'\\', b'#'], FPARSELN_UNESCREST, None).unwrap();
    assert_eq!(line, b"a\\#bnc\\\\dqe");
}

#[test]
fn no_comment_character_keeps_everything() {
    let mut source = Lines::new(b"# not a comment\n");
    let line = fparseln(&mut source, [b'\\', b'\\', 0], 0, None).unwrap();
    assert_eq!(line, b"# not a comment");
}

#[test]
fn custom_delimiters() {
    let mut source = Lines::new(b"a;b\nc!d\n");
    let first = fparseln(&mut source, [b'!', b'!', b';'], 0, None).unwrap();
    assert_eq!(first, b"a");
    let second = fparseln(&mut source, [b'!', b'!', b';'], 0, None).unwrap();
    assert_eq!(second, b"c!d");
}

#[test]
fn line_over_the_cap_is_rejected() {
    let mut source = Lines::new(&alloc::vec![b'a'; LINE_CAP.wrapping_add(1)]);
    assert!(fparseln(&mut source, [b'\\', b'\\', b'#'], 0, None).is_none());
}
