// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Assertions for the mailbox envelope.

use super::*;

/// A synthetic layout for the tests: 8-bit magic `0x5a`, 4-bit version 2, 4-bit status, 8-bit
/// sequence and 4-bit length, with a trailing checksum word. It is a test vehicle, not a claim
/// about the real RP1 mailbox.
const LAYOUT: Layout = Layout {
    magic: 0x5A,
    magic_mask: 0xFF,
    version: 2,
    version_shift: 8,
    version_mask: 0xF,
    sequence_shift: 16,
    sequence_mask: 0xFF,
    length_shift: 24,
    length_mask: 0xF,
    status_shift: 12,
    status_mask: 0xF,
    checksum: true,
};

fn header(status: u32, sequence: u32, length: u32) -> u32 {
    0x5A | (2 << 8) | (status << 12) | (sequence << 16) | (length << 24)
}

fn message(header: u32, payload: &[u32]) -> Vec<u32> {
    let mut words = vec![header];
    words.extend_from_slice(payload);
    let mut checksum = header;
    for word in payload {
        checksum ^= *word;
    }
    words.push(checksum & 0xFFFF);
    words
}

#[test]
fn decodes_a_well_formed_response() {
    let words = message(header(3, 7, 2), &[0x1111_2222, 0x3333_4444]);
    let frame = decode(&words, &LAYOUT).expect("frame");
    assert_eq!(
        frame,
        Frame {
            sequence: 7,
            length: 2,
            status: 3
        }
    );
}

#[test]
fn an_empty_response_is_rejected() {
    assert_eq!(decode(&[], &LAYOUT), Err(FrameError::Empty));
}

#[test]
fn a_bad_magic_is_rejected() {
    let words = message(header(0, 1, 0) ^ 0x01, &[]);
    assert_eq!(
        decode(&words, &LAYOUT),
        Err(FrameError::BadMagic {
            header: header(0, 1, 0) ^ 1
        })
    );
}

#[test]
fn a_bad_version_is_rejected() {
    let words = message(header(0, 1, 0) & !0xF00, &[]);
    assert_eq!(
        decode(&words, &LAYOUT),
        Err(FrameError::BadVersion { version: 0 })
    );
}

#[test]
fn an_oversized_length_is_rejected() {
    let mut words = message(header(0, 0, 0), &[]);
    if let Some(word) = words.first_mut() {
        *word |= 0xF << 24;
    }
    assert_eq!(
        decode(&words, &LAYOUT),
        Err(FrameError::LengthTooLarge { length: 15 })
    );
}

#[test]
fn a_truncated_payload_is_rejected() {
    let words = vec![header(0, 1, 2), 0xAAAA_AAAA];
    assert_eq!(
        decode(&words, &LAYOUT),
        Err(FrameError::Truncated {
            length: 2,
            words: 2
        })
    );
}

#[test]
fn a_bad_checksum_is_rejected() {
    let mut words = message(header(0, 1, 1), &[0x1234_5678]);
    if let Some(last) = words.last_mut() {
        *last ^= 0xFFFF;
    }
    assert!(matches!(
        decode(&words, &LAYOUT),
        Err(FrameError::Checksum { .. })
    ));
}

#[test]
fn a_missing_checksum_word_is_rejected() {
    let words = vec![header(0, 1, 0)];
    assert_eq!(
        decode(&words, &LAYOUT),
        Err(FrameError::Truncated {
            length: 0,
            words: 1
        })
    );
}

#[test]
fn a_layout_without_checksum_accepts_a_header_only_message() {
    let layout = Layout {
        checksum: false,
        ..LAYOUT
    };
    let frame = decode(&[header(0, 2, 0)], &layout).expect("frame");
    assert_eq!(frame.sequence, 2);
    assert_eq!(frame.length, 0);
}

#[test]
fn an_unusable_layout_is_rejected() {
    let mut layout = LAYOUT;
    layout.magic_mask = 0;
    assert_eq!(layout.validate(), Err(FrameError::Layout));
    assert_eq!(decode(&[0], &layout), Err(FrameError::Layout));

    let mut layout = LAYOUT;
    layout.sequence_mask = 0x1_0000;
    assert_eq!(layout.validate(), Err(FrameError::Layout));

    let mut layout = LAYOUT;
    layout.length_shift = 32;
    assert_eq!(layout.validate(), Err(FrameError::Layout));

    let mut layout = LAYOUT;
    layout.status_mask = 0x0010_0000;
    assert_eq!(layout.validate(), Err(FrameError::Layout));
}
