// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! C70 finding: `inet_pton` must expand a leading `::` correctly.

use raven_libc_resolv::{inet_ntop, inet_pton};

#[test]
fn double_colon_expands_to_the_documented_address() {
    let mut address = [0u8; 16];
    assert_eq!(inet_pton(10, b"::a:4e:3:e:a:a", &mut address), 1);
    assert_eq!(
        address,
        [
            0, 0, 0, 0, 0, 0x0a, 0, 0x4e, 0, 0x03, 0, 0x0e, 0, 0x0a, 0, 0x0a
        ]
    );
}

#[test]
fn the_minimized_regression_input_round_trips() {
    // Minimized fuzz input: selector 0x3a (58 % 3 == 1 -> AF_INET6), then the text. The
    // checked-in .bin is 16 bytes; the 2026-11-13 note and the C74k ticket call it 17.
    let input = [
        0x3a, b'0', b':', b':', b'a', b':', b'4', b'e', b':', b'3', b':', b'e', b':', b'a', b':',
        b'a',
    ];
    assert_eq!(input.len(), 16);
    assert_eq!(input[0] % 3, 1);
    let text = input.get(1..).expect("text");
    let expected = [
        0, 0, 0, 0, 0, 0x0a, 0, 0x4e, 0, 0x03, 0, 0x0e, 0, 0x0a, 0, 0x0a,
    ];

    let mut address = [0u8; 16];
    assert_eq!(inet_pton(10, text, &mut address), 1);
    assert_eq!(address, expected);

    let mut rendered = [0u8; 64];
    let length = inet_ntop(10, &address, &mut rendered).expect("inet_ntop");
    assert_eq!(rendered.get(..length), Some(b"::a:4e:3:e:a:a".as_slice()));

    let mut again = [0u8; 16];
    assert_eq!(
        inet_pton(10, rendered.get(..length).expect("length"), &mut again),
        1
    );
    assert_eq!(again, expected);
}
