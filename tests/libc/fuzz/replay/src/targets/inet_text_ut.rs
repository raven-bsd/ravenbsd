// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

use super::*;

#[test]
fn inet_text_hostile_ok() {
    body(b"\x00\xff\xff\xff\xff");
    body(b"\x01::ffff:1.2.3.4");
    body(b"\x02 256.1.1.1");
}

#[test]
fn inet_text_boundaries_ok() {
    for text in [
        b"".as_slice(),
        b"::",
        b"::::",
        b"1:2:3:4:5:6:7:8:9",
        b"1.2.3.4.5",
        b"256.1.1.1",
        b"0x",
        b"::ffff:1.2.3.4",
        b"1::2::3",
        b"ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff",
    ] {
        let mut input = vec![0u8];
        input.extend_from_slice(text);
        body(&input);
        let mut input = vec![1u8];
        input.extend_from_slice(text);
        body(&input);
    }
}

#[test]
#[should_panic(expected = "P-inet-2")]
fn inet_text_mutation_2_fires() {
    let mut broken = [0u8; 4];
    let result = inet_pton(AF_INET, b"1.2.3.4", &mut broken);
    let forged = result + 7;
    assert!(
        forged == 1 || forged == 0 || forged == -1,
        "P-inet-2: inet_pton returned {forged}"
    );
}

#[test]
fn every_family_selector_runs() {
    for selector in 0u8..8 {
        let mut input = vec![selector];
        input.extend_from_slice(b"::1");
        body(&input);
    }
}

#[test]
fn legacy_forms_are_accepted() {
    assert!(inet_aton(b"1").is_some());
    assert!(inet_aton(b"1.2").is_some());
    assert!(inet_aton(b"1.2.3").is_some());
    assert!(inet_aton(b"1.2.3.4").is_some());
    assert!(inet_aton(b"1.2.3.4.5").is_none());
}
