// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#![allow(
    clippy::arithmetic_side_effects,
    clippy::indexing_slicing,
    clippy::panic,
    clippy::unwrap_used,
    clippy::expect_used
)]

use super::offset;

#[test]
fn ticket_vectors() {
    assert_eq!(offset(b""), 0);
    assert_eq!(offset(b"a"), 0);
    assert_eq!(offset(b"a/"), 2);
    assert_eq!(offset(b"/a"), 1);
    assert_eq!(offset(b"/"), 1);
    assert_eq!(offset(b"a/b"), 2);
    assert_eq!(offset(b"a//"), 3);
}

#[test]
fn returns_the_text_after_the_last_slash() {
    let path = b"/usr/local/bin/tool";
    let start = offset(path);
    assert_eq!(&path[start..], b"tool");
}

#[test]
fn every_short_path() {
    let alphabet = *b"a/";
    let mut strings = vec![Vec::new()];
    for _ in 0..5 {
        let mut next = Vec::new();
        for prefix in &strings {
            for &byte in &alphabet {
                let mut value = prefix.clone();
                value.push(byte);
                next.push(value);
            }
        }
        strings.extend(next.iter().cloned());
    }
    for path in &strings {
        let expected = path
            .iter()
            .rposition(|&byte| byte == b'/')
            .map_or(0, |index| index + 1);
        assert_eq!(offset(path), expected, "{path:?}");
    }
}
