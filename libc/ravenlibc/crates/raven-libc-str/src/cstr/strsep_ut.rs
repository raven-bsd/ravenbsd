// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#![allow(
    clippy::arithmetic_side_effects,
    clippy::indexing_slicing,
    clippy::panic,
    clippy::unwrap_used,
    clippy::expect_used
)]

use super::strsep;

fn fields(input: &[u8], delim: &[u8]) -> Vec<Vec<u8>> {
    let mut buffer = input.to_vec();
    buffer.push(0);
    let mut position = Some(0usize);
    let mut collected = Vec::new();
    while let Some((start, len)) = strsep(&mut buffer, &mut position, delim) {
        collected.push(buffer[start..start + len].to_vec());
    }
    collected
}

#[test]
fn empty_fields_are_returned() {
    assert_eq!(
        fields(b",,a,b;;c", b",;"),
        vec![
            Vec::new(),
            Vec::new(),
            b"a".to_vec(),
            b"b".to_vec(),
            Vec::new(),
            b"c".to_vec()
        ]
    );
}

#[test]
fn empty_input_and_only_delimiters() {
    assert_eq!(fields(b"", b","), vec![Vec::new()]);
    assert_eq!(
        fields(b",;", b",;"),
        vec![Vec::new(), Vec::new(), Vec::new()]
    );
}

#[test]
fn trailing_delimiter_gives_an_empty_last_field() {
    assert_eq!(fields(b"a,", b","), vec![b"a".to_vec(), Vec::new()]);
}

#[test]
fn empty_delimiter_set() {
    assert_eq!(fields(b"a,b", b""), vec![b"a,b".to_vec()]);
}

#[test]
fn position_becomes_none_at_the_terminator() {
    let mut buffer = b"a\0".to_vec();
    let mut position = Some(0usize);
    assert_eq!(strsep(&mut buffer, &mut position, b","), Some((0, 1)));
    assert_eq!(position, None);
    assert_eq!(strsep(&mut buffer, &mut position, b","), None);
}

#[test]
fn writes_nul_over_the_delimiter() {
    let mut buffer = b"ab,cd\0".to_vec();
    let mut position = Some(0usize);
    assert_eq!(strsep(&mut buffer, &mut position, b","), Some((0, 2)));
    assert_eq!(&buffer[..6], b"ab\0cd\0");
    assert_eq!(position, Some(3));
}

#[test]
fn every_string_over_two_delimiters() {
    let alphabet = *b"a,;";
    for first in alphabet {
        for second in alphabet {
            for third in alphabet {
                let input = [first, second, third];
                let collected = fields(&input, b",;");
                let expected: Vec<Vec<u8>> = input
                    .split(|byte| *byte == b',' || *byte == b';')
                    .map(|field| field.to_vec())
                    .collect();
                assert_eq!(collected, expected, "{input:?}");
            }
        }
    }
}
