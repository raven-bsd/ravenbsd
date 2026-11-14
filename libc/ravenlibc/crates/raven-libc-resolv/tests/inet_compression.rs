// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! C70 FINDING 1: the compressed `::` expansion in `inet_pton(AF_INET6, ...)`.
//!
//! The minimized 16-byte C70 input starts with the fuzz selector byte `0x3a` (selector 1 =
//! `AF_INET6`) followed by `0::a:4e:3:e:a:a`. The direct tests below cover that address, the
//! equivalent leading-`::` spelling and the adjacent one-group/two-group forms. Every successful
//! parse must round-trip through `inet_ntop` back to the same 16 bytes.

use raven_libc_resolv::consts::{AF_INET, AF_INET6};
use raven_libc_resolv::{inet_ntop, inet_pton};

fn address(groups: [u16; 8]) -> [u8; 16] {
    let mut out = [0u8; 16];
    for (index, group) in groups.iter().enumerate() {
        let at = index * 2;
        out[at] = (group >> 8) as u8;
        out[at + 1] = (group & 0xff) as u8;
    }
    out
}

fn parse(text: &[u8]) -> [u8; 16] {
    let mut out = [0u8; 16];
    assert_eq!(inet_pton(AF_INET6, text, &mut out), 1, "{text:?}");
    out
}

fn round_trip(text: &[u8], expected: [u8; 16]) {
    let parsed = parse(text);
    assert_eq!(parsed, expected, "parse {text:?}");
    let mut rendered = [0u8; 64];
    let length = inet_ntop(AF_INET6, &parsed, &mut rendered).expect("inet_ntop");
    let again = parse(rendered.get(..length).expect("length"));
    assert_eq!(again, expected, "round trip {text:?}");
}

#[test]
fn c70_minimized_input_round_trips() {
    // The fuzz input: selector 0x3a (58 % 3 == 1 -> AF_INET6), then the text.
    let input = [
        0x3a, 0x30, 0x3a, 0x3a, 0x61, 0x3a, 0x34, 0x65, 0x3a, 0x33, 0x3a, 0x65, 0x3a, 0x61, 0x3a,
        0x61,
    ];
    let selector = input[0] % 3;
    assert_eq!(selector, 1);
    let text = input.get(1..).expect("text");
    let expected = address([0, 0, 0x000a, 0x004e, 0x0003, 0x000e, 0x000a, 0x000a]);
    round_trip(text, expected);
}

#[test]
fn leading_double_colon_matches_the_zero_group_form() {
    let expected = address([0, 0, 0x000a, 0x004e, 0x0003, 0x000e, 0x000a, 0x000a]);
    round_trip(b"0::a:4e:3:e:a:a", expected);
    round_trip(b"::a:4e:3:e:a:a", expected);
}

#[test]
fn adjacent_one_and_two_group_compressions() {
    round_trip(b"0::a", address([0, 0, 0, 0, 0, 0, 0, 0x000a]));
    round_trip(b"0::a:4e", address([0, 0, 0, 0, 0, 0, 0x000a, 0x004e]));
    round_trip(
        b"0::a:4e:3",
        address([0, 0, 0, 0, 0, 0x000a, 0x004e, 0x0003]),
    );
    round_trip(
        b"0::a:4e:3:e",
        address([0, 0, 0, 0, 0x000a, 0x004e, 0x0003, 0x000e]),
    );
    round_trip(
        b"0::a:4e:3:e:a",
        address([0, 0, 0, 0x000a, 0x004e, 0x0003, 0x000e, 0x000a]),
    );
    round_trip(
        b"0::a:4e:3:e:a:a",
        address([0, 0, 0x000a, 0x004e, 0x0003, 0x000e, 0x000a, 0x000a]),
    );
}

#[test]
fn terminal_and_middle_compressions() {
    round_trip(b"::", [0u8; 16]);
    round_trip(b"::1", address([0, 0, 0, 0, 0, 0, 0, 0x0001]));
    round_trip(b"1::", address([0x0001, 0, 0, 0, 0, 0, 0, 0]));
    round_trip(b"1::2", address([0x0001, 0, 0, 0, 0, 0, 0, 0x0002]));
    round_trip(b"1:2::3", address([0x0001, 0x0002, 0, 0, 0, 0, 0, 0x0003]));
    round_trip(
        b"1:2::3:4",
        address([0x0001, 0x0002, 0, 0, 0, 0, 0x0003, 0x0004]),
    );
}

#[test]
fn invalid_compressions_are_still_rejected() {
    for text in [
        b"1::2::3".as_slice(),
        b"1:2:3:4:5:6:7:8:9",
        b"1:2:3:4:5:6:7",
        b"::1:2:3:4:5:6:7:8",
        b"1:::2",
        b":1::2",
        b"::a:4e:3:e:a:a:a:a",
    ] {
        let mut out = [0u8; 16];
        assert_eq!(inet_pton(AF_INET6, text, &mut out), 0, "{text:?}");
    }
}

#[test]
fn ipv4_still_works() {
    let mut out = [0u8; 16];
    assert_eq!(inet_pton(AF_INET, b"192.0.2.1", &mut out), 1);
    assert_eq!(&out[..4], &[192, 0, 2, 1]);
}
