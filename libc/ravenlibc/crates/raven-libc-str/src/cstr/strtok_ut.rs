// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#![allow(
    clippy::arithmetic_side_effects,
    clippy::indexing_slicing,
    clippy::panic,
    clippy::unwrap_used,
    clippy::expect_used
)]

use super::strtok;

fn tokens(input: &[u8], delim: &[u8]) -> Vec<Vec<u8>> {
    let mut buffer = input.to_vec();
    buffer.push(0);
    let mut cursor = Some(0usize);
    let mut collected = Vec::new();
    while let Some((start, len)) = strtok(&mut buffer, &mut cursor, delim) {
        collected.push(buffer[start..start + len].to_vec());
    }
    collected
}

#[test]
fn skips_leading_and_repeated_delimiters() {
    assert_eq!(
        tokens(b",,a,b;;c", b",;"),
        vec![b"a".to_vec(), b"b".to_vec(), b"c".to_vec()]
    );
}

#[test]
fn empty_input_and_only_delimiters() {
    assert!(tokens(b"", b",").is_empty());
    assert!(tokens(b",;,", b",;").is_empty());
}

#[test]
fn trailing_delimiter() {
    assert_eq!(tokens(b"a,", b","), vec![b"a".to_vec()]);
}

#[test]
fn empty_delimiter_set() {
    assert_eq!(tokens(b"a,b", b""), vec![b"a,b".to_vec()]);
    assert!(tokens(b"", b"").is_empty());
}

#[test]
fn cursor_becomes_none_after_the_last_token() {
    let mut buffer = b"a\0".to_vec();
    let mut cursor = Some(0usize);
    assert_eq!(strtok(&mut buffer, &mut cursor, b","), Some((0, 1)));
    assert_eq!(cursor, None);
    assert_eq!(strtok(&mut buffer, &mut cursor, b","), None);
}

#[test]
fn two_cursors_interleave() {
    let mut first = b"a,b,c\0".to_vec();
    let mut second = b"1;2;3\0".to_vec();
    let mut cursor_first = Some(0usize);
    let mut cursor_second = Some(0usize);
    let mut first_tokens = Vec::new();
    let mut second_tokens = Vec::new();
    loop {
        let one = strtok(&mut first, &mut cursor_first, b",");
        let two = strtok(&mut second, &mut cursor_second, b";");
        if let Some((start, len)) = one {
            first_tokens.push(first[start..start + len].to_vec());
        }
        if let Some((start, len)) = two {
            second_tokens.push(second[start..start + len].to_vec());
        }
        if one.is_none() && two.is_none() {
            break;
        }
    }
    assert_eq!(
        first_tokens,
        vec![b"a".to_vec(), b"b".to_vec(), b"c".to_vec()]
    );
    assert_eq!(
        second_tokens,
        vec![b"1".to_vec(), b"2".to_vec(), b"3".to_vec()]
    );
}

#[test]
fn every_two_byte_string_with_two_delimiters() {
    let alphabet = *b"a,;";
    for first in alphabet {
        for second in alphabet {
            let input = [first, second];
            let collected = tokens(&input, b",;");
            let mut expected: Vec<Vec<u8>> = Vec::new();
            let mut current: Vec<u8> = Vec::new();
            let mut started = false;
            for &byte in &input {
                if byte == b',' || byte == b';' {
                    if started {
                        expected.push(current.clone());
                        current.clear();
                        started = false;
                    }
                } else {
                    current.push(byte);
                    started = true;
                }
            }
            if started {
                expected.push(current);
            }
            assert_eq!(collected, expected, "{input:?}");
        }
    }
}
