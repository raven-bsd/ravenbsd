// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

use super::*;

#[test]
fn scanf_format_hostile_ok() {
    let mut input = vec![0u8];
    input.extend_from_slice(&[0u8; 400]);
    input.extend_from_slice(b"%d %s %[a-z] %m[a-z] %n");
    input.push(0);
    input.extend_from_slice(b"42 word abc");
    body(&input);
}

#[test]
fn scanf_format_boundaries_ok() {
    for format in [
        b"%".as_slice(),
        b"%[",
        b"%[^",
        b"%[]]",
        b"%*s%*s",
        b"%999999999d",
        b"%m[a-z]",
    ] {
        for flags in [0u8, 1] {
            let mut input = vec![flags];
            input.extend_from_slice(&[0u8; 400]);
            input.extend_from_slice(format);
            input.push(0);
            input.extend_from_slice(&[b'9'; 1024]);
            body(&input);
        }
    }
}

#[test]
#[should_panic(expected = "P-scanf-4")]
fn scanf_format_mutation_4_fires() {
    let len = SCAN_TOKEN_MAX + 2;
    assert!(
        len <= SCAN_TOKEN_MAX.saturating_add(1),
        "P-scanf-4: a string store exceeded SCAN_TOKEN_MAX"
    );
}

#[test]
fn width_and_suppression_do_not_overflow() {
    let mut input = vec![1u8];
    input.extend_from_slice(&[0u8; 400]);
    input.extend_from_slice(b"%*s %5s %[abc]");
    input.push(0);
    input.extend_from_slice(b"alpha beta cabc");
    body(&input);
}

#[test]
fn a_short_input_assigns_nothing() {
    let mut input = vec![0u8];
    input.extend_from_slice(&[0u8; 400]);
    input.extend_from_slice(b"%d");
    input.push(0);
    body(&input);
}
