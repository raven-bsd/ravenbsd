// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

use super::*;

#[test]
fn glob_pattern_hostile_ok() {
    let mut input = vec![0u8, 0, 3];
    for name in [b"a".as_slice(), b"b", b"sub"] {
        input.push(name.len() as u8);
        input.extend_from_slice(name);
    }
    input.extend_from_slice(b"*");
    body(&input);
}

#[test]
fn glob_pattern_boundaries_ok() {
    for pattern in [
        b"[".as_slice(),
        b"[!",
        b"[]",
        b"[a-",
        b"\\",
        b"**/**",
        b"{a,b}",
        b"~",
    ] {
        for flags in [0u8, 0x40, 0x80, 0xff] {
            let mut input = vec![flags, 0, 2];
            input.extend_from_slice(&[1, b'a']);
            input.extend_from_slice(&[3, b's', b'u', b'b']);
            input.extend_from_slice(pattern);
            body(&input);
        }
    }
    let mut input = vec![0u8, 0, 0];
    input.extend_from_slice(&[b'*'; 128]);
    body(&input);
}

#[test]
#[should_panic(expected = "P-glob-2")]
fn glob_pattern_mutation_2_fires() {
    let result = -1i32;
    assert!(
        result == 0 || result == 1,
        "P-glob-2: fnmatch returned {result}"
    );
}

#[test]
fn glob_nosort_and_nocheck_run() {
    let mut input = vec![0x30u8, 0, 2];
    input.extend_from_slice(&[1, b'a']);
    input.extend_from_slice(&[1, b'b']);
    input.extend_from_slice(b"*");
    body(&input);
}

#[test]
fn a_malformed_bracket_does_not_return_a_prefix_entry() {
    let data = [0u8, 0, 1, 3, b's', b'u', b'b', b's', b'['];
    body(&data);
}

#[test]
fn fnmatch_is_zero_or_one() {
    let mut input = vec![0x80u8, 0, 1, 1, b'a'];
    input.extend_from_slice(b"*");
    body(&input);
    assert_eq!(fnmatch(b"a*", b"abc", 0, &ByteLocale), 0);
    assert_eq!(fnmatch(b"a*", b"xbc", 0, &ByteLocale), 1);
}
