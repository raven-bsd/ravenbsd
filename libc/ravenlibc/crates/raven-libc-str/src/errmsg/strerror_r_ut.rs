// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#![allow(
    clippy::arithmetic_side_effects,
    clippy::indexing_slicing,
    clippy::panic,
    clippy::unwrap_used,
    clippy::expect_used
)]

use super::{Xsi, strerror_r_xsi};

fn run(e: i32, len: usize) -> (Xsi, Vec<u8>) {
    let mut buffer = vec![0xAAu8; len];
    let result = strerror_r_xsi(e, &mut buffer);
    (result, buffer)
}

#[test]
fn known_message_fits() {
    let (result, buffer) = run(22, 32);
    assert_eq!(result, Xsi::Ok);
    assert_eq!(&buffer[..17], b"Invalid argument\0");
    assert_eq!(buffer[17], 0xAA);
}

#[test]
fn every_buffer_length_for_a_known_message() {
    let text = b"Invalid argument";
    for len in [0usize, 1, 2, text.len() - 1, text.len(), text.len() + 1, 64] {
        let (result, buffer) = run(22, len);
        if len == 0 {
            assert_eq!(result, Xsi::Erange);
            assert!(buffer.is_empty());
        } else if len <= text.len() {
            assert_eq!(result, Xsi::Erange, "len {len}");
            assert_eq!(buffer[len - 1], 0, "len {len}");
            assert_eq!(&buffer[..len - 1], &text[..len - 1], "len {len}");
        } else {
            assert_eq!(result, Xsi::Ok, "len {len}");
            assert_eq!(&buffer[..text.len()], text, "len {len}");
            assert_eq!(buffer[text.len()], 0, "len {len}");
            if text.len() + 1 < len {
                assert_eq!(buffer[text.len() + 1], 0xAA, "len {len}");
            }
        }
    }
}

#[test]
fn unknown_number_is_einval_with_text() {
    let (result, buffer) = run(1000, 32);
    assert_eq!(result, Xsi::Einval);
    let expected = b"Unknown error: 1000";
    assert_eq!(&buffer[..expected.len()], expected);
    assert_eq!(buffer[expected.len()], 0);
}

#[test]
fn unknown_number_with_zero_length_still_einval() {
    let (result, buffer) = run(1000, 0);
    assert_eq!(result, Xsi::Einval);
    assert!(buffer.is_empty());
}

#[test]
fn unknown_number_truncates_with_nul() {
    let (result, buffer) = run(1000, 4);
    assert_eq!(result, Xsi::Einval);
    assert_eq!(&buffer, b"Unk\0");
}

#[test]
fn negative_number() {
    let (result, buffer) = run(-1, 64);
    assert_eq!(result, Xsi::Einval);
    assert_eq!(&buffer[..18], b"Unknown error: -1\0");
}

#[test]
fn two_calls_share_nothing() {
    let (first, first_buffer) = run(2, 32);
    let (second, second_buffer) = run(13, 32);
    assert_eq!(first, Xsi::Ok);
    assert_eq!(second, Xsi::Ok);
    assert_eq!(&first_buffer[..26], b"No such file or directory\0");
    assert_eq!(&second_buffer[..18], b"Permission denied\0");
}

#[test]
fn every_errno_and_length() {
    for e in -3i32..=200 {
        for len in [0usize, 1, 2, 26, 63, 64, 65] {
            let (result, buffer) = run(e, len);
            if (0..=133).contains(&e) {
                assert_ne!(result, Xsi::Einval, "known e {e}");
            } else if len != 0 {
                assert_eq!(result, Xsi::Einval, "unknown e {e}");
            }
            if len != 0 {
                let nul = buffer.iter().position(|&byte| byte == 0);
                assert!(nul.is_some(), "e {e} len {len}: no NUL");
                let nul = nul.unwrap();
                assert!(nul < len - 1 || buffer[len - 1] == 0, "e {e} len {len}");
                match result {
                    Xsi::Ok => {
                        if nul + 1 < len {
                            assert_eq!(buffer[nul + 1], 0xAA, "e {e} len {len}: guard touched");
                        }
                    }
                    Xsi::Erange | Xsi::Einval => {}
                }
            }
        }
    }
}
