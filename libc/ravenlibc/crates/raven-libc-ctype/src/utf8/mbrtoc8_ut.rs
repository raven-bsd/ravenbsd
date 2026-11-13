// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
use super::*;

#[test]
fn two_byte_character_yields_lead_then_continuation() {
    let mut state = MbState::INITIAL;
    assert_eq!(
        mbrtoc8(&mut state, MbMode::Utf8, &[0xc3, 0xa4]),
        Mbr8::Char {
            unit: 0xc3,
            used: 2
        }
    );
    assert_eq!(
        mbrtoc8(&mut state, MbMode::Utf8, &[]),
        Mbr8::Continuation { unit: 0xa4 }
    );
    assert!(state.is_initial());
}

#[test]
fn four_byte_character_yields_all_four_units() {
    let mut state = MbState::INITIAL;
    assert_eq!(
        mbrtoc8(&mut state, MbMode::Utf8, &[0xf0, 0x9f, 0x92, 0xa9]),
        Mbr8::Char {
            unit: 0xf0,
            used: 4
        }
    );
    for expected in [0x9f, 0x92, 0xa9] {
        assert_eq!(
            mbrtoc8(&mut state, MbMode::Utf8, &[]),
            Mbr8::Continuation { unit: expected }
        );
    }
    assert!(state.is_initial());
}

#[test]
fn byte_mode_encodes_code_units_as_utf8() {
    let mut state = MbState::INITIAL;
    assert_eq!(
        mbrtoc8(&mut state, MbMode::Byte, &[0xe9]),
        Mbr8::Char {
            unit: 0xed,
            used: 1
        }
    );
    assert_eq!(
        mbrtoc8(&mut state, MbMode::Byte, &[]),
        Mbr8::Continuation { unit: 0xbf }
    );
    assert_eq!(
        mbrtoc8(&mut state, MbMode::Byte, &[]),
        Mbr8::Continuation { unit: 0xa9 }
    );
}

#[test]
fn invalid_sequences_are_ilseq() {
    let mut state = MbState::INITIAL;
    assert_eq!(mbrtoc8(&mut state, MbMode::Utf8, &[0xff]), Mbr8::Ilseq);
    assert!(state.is_initial());
}
