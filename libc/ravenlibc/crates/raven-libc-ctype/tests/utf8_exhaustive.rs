// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Exhaustive UTF-8 acceptance and encoder tests.

use raven_libc_ctype::*;

macro_rules! same {
    ($left:expr, $right:expr $(,)?) => {
        assert!($left == $right, "{} != {}", stringify!($left), stringify!($right))
    };
    ($left:expr, $right:expr, $($arg:tt)+) => {
        assert!($left == $right, $($arg)+)
    };
}

fn check_acceptance(bytes: &[u8]) {
    let expected = core::str::from_utf8(bytes);
    let mut state = MbState::INITIAL;
    let actual = mbrtowc(&mut state, MbMode::Utf8, bytes);
    match expected {
        Ok(text) => {
            let first = text
                .chars()
                .next()
                .expect("from_utf8 returned an empty string");
            same!(
                actual,
                Mbr::Char {
                    wc: first as u32,
                    used: first.len_utf8()
                },
                "bytes {bytes:02x?}"
            );
        }
        Err(_) => {
            if let Mbr::Char { used, .. } = actual {
                assert!(
                    used < bytes.len(),
                    "accepted invalid bytes {bytes:02x?} with used={used}"
                );
            }
            assert!(
                !matches!(actual, Mbr::Char { wc: 0xFFFD, .. }),
                "produced a replacement character for {bytes:02x?}"
            );
        }
    }
}

#[test]
fn one_byte_sequences() {
    for byte in 0..=255u8 {
        check_acceptance(&[byte]);
    }
}

#[test]
fn two_byte_sequences() {
    for first in 0..=255u8 {
        for second in 0..=255u8 {
            check_acceptance(&[first, second]);
        }
    }
}

#[test]
fn three_byte_sequences() {
    for first in 0..=255u8 {
        for second in 0..=255u8 {
            for third in 0..=255u8 {
                check_acceptance(&[first, second, third]);
            }
        }
    }
}

#[test]
#[cfg_attr(
    debug_assertions,
    ignore = "8 x 16,777,216 four-byte sequences; run with --release"
)]
fn four_byte_sequences_with_every_lead_f0_to_f7() {
    for lead in 0xf0..=0xf7u8 {
        for second in 0..=255u8 {
            for third in 0..=255u8 {
                for fourth in 0..=255u8 {
                    check_acceptance(&[lead, second, third, fourth]);
                }
            }
        }
    }
}

#[test]
fn four_byte_boundaries_are_checked_in_every_mode() {
    for lead in 0x00..=0xefu8 {
        for fourth in [0x00u8, 0x7f, 0x80, 0xbf, 0xc0, 0xff] {
            let mut bytes = [0u8; 4];
            bytes[0] = lead;
            bytes[1] = 0x80;
            bytes[2] = 0x80;
            bytes[3] = fourth;
            check_acceptance(&bytes);
        }
    }
    for tail in [
        [0x80, 0x80, 0x80],
        [0x90, 0x80, 0x80],
        [0x8f, 0xbf, 0xbf],
        [0x90, 0x80, 0x80],
        [0x8f, 0xbf, 0xbf],
        [0x80, 0x80, 0x80],
    ] {
        check_acceptance(&[0xf0, tail[0], tail[1], tail[2]]);
        check_acceptance(&[0xf4, tail[0], tail[1], tail[2]]);
    }
}

#[test]
fn resumability_of_two_byte_accepted_sequences() {
    for first in 0..=255u8 {
        for second in 0..=255u8 {
            let bytes = [first, second];
            let mut whole = MbState::INITIAL;
            let expected = mbrtowc(&mut whole, MbMode::Utf8, &bytes);
            let Mbr::Char { wc, used } = expected else {
                continue;
            };
            if used != 2 {
                continue;
            }
            let mut state = MbState::INITIAL;
            same!(mbrtowc(&mut state, MbMode::Utf8, &[first]), Mbr::Incomplete);
            assert!(!mbsinit(&state));
            same!(
                mbrtowc(&mut state, MbMode::Utf8, &[second]),
                Mbr::Char { wc, used: 1 }
            );
            assert!(mbsinit(&state));
        }
    }
}

#[test]
fn resumability_of_three_and_four_byte_samples() {
    for bytes in [
        &[0xe2, 0x82, 0xac][..],
        &[0xef, 0xbf, 0xbf][..],
        &[0xf0, 0x9f, 0x92, 0xa9][..],
        &[0xf4, 0x8f, 0xbf, 0xbf][..],
    ] {
        let mut whole = MbState::INITIAL;
        let Mbr::Char { wc, used } = mbrtowc(&mut whole, MbMode::Utf8, bytes) else {
            panic!("sample rejected: {bytes:02x?}");
        };
        same!(used, bytes.len());
        for split in 1..bytes.len() {
            let mut state = MbState::INITIAL;
            same!(
                mbrtowc(&mut state, MbMode::Utf8, &bytes[..split]),
                Mbr::Incomplete
            );
            same!(
                mbrtowc(&mut state, MbMode::Utf8, &bytes[split..]),
                Mbr::Char {
                    wc,
                    used: bytes.len() - split
                }
            );
            assert!(mbsinit(&state));
        }
    }
}

#[test]
fn encoder_round_trips_every_scalar() {
    for cp in 0..=0x10ffffu32 {
        if (0xd800..=0xdfff).contains(&cp) {
            continue;
        }
        let mut out = [0u8; 4];
        let Ok(length) = wcrtomb(MbMode::Utf8, cp, &mut out) else {
            panic!("scalar U+{cp:04X} did not encode");
        };
        let mut state = MbState::INITIAL;
        same!(
            mbrtowc(&mut state, MbMode::Utf8, &out[..length]),
            Mbr::Char {
                wc: cp,
                used: length
            }
        );
    }
}

#[test]
fn encoder_rejects_surrogates_and_out_of_range() {
    for cp in [0xd800, 0xdfff, 0x110000, 0x8000_0000, u32::MAX] {
        let mut out = [0u8; 4];
        same!(wcrtomb(MbMode::Utf8, cp, &mut out), Err(MbError::Ilseq));
    }
}

#[test]
fn encoder_length_boundaries() {
    for (cp, length) in [
        (0x7f, 1),
        (0x80, 2),
        (0x7ff, 2),
        (0x800, 3),
        (0xffff, 3),
        (0x10000, 4),
        (0x10ffff, 4),
    ] {
        let mut out = [0u8; 4];
        same!(wcrtomb(MbMode::Utf8, cp, &mut out), Ok(length));
    }
}

#[test]
fn byte_mode_is_byte_transparent() {
    for byte in 0..=255u8 {
        let mut state = MbState::INITIAL;
        let expected = if byte < 0x80 {
            u32::from(byte)
        } else {
            0xdf00 | u32::from(byte)
        };
        same!(
            mbrtowc(&mut state, MbMode::Byte, &[byte]),
            Mbr::Char {
                wc: expected,
                used: 1
            }
        );
        let mut out = [0u8; 4];
        same!(wcrtomb(MbMode::Byte, expected, &mut out), Ok(1));
        same!(out[0], byte);
        same!(btowc(MbMode::Byte, i32::from(byte)), expected);
        same!(wctob(MbMode::Byte, expected), i32::from(byte));
    }
    same!(MbMode::Byte.mb_cur_max(), 1);
    same!(MbMode::Utf8.mb_cur_max(), 4);
}

#[test]
fn negative_sequences_have_exact_outcomes() {
    let mut state = MbState::INITIAL;
    same!(
        mbrtowc(&mut state, MbMode::Utf8, &[0xe0, 0x80, 0x80]),
        Mbr::Ilseq
    );
    same!(
        mbrtowc(&mut state, MbMode::Utf8, &[0xed, 0xa0, 0x80]),
        Mbr::Ilseq
    );
    same!(
        mbrtowc(&mut state, MbMode::Utf8, &[0xf4, 0x90, 0x80, 0x80]),
        Mbr::Ilseq
    );
    same!(mbrtowc(&mut state, MbMode::Utf8, &[0xc0, 0x80]), Mbr::Ilseq);
    same!(mbrtowc(&mut state, MbMode::Utf8, &[0xff]), Mbr::Ilseq);
    same!(
        mbrtowc(&mut state, MbMode::Utf8, &[0xe2, 0x82]),
        Mbr::Incomplete
    );
    assert!(state.opaque1 != 0);
    same!(mbrtowc(&mut state, MbMode::Utf8, &[]), Mbr::Incomplete);
    assert!(state.opaque1 != 0);
}
