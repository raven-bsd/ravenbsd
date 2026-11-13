// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
use super::*;

#[test]
fn surrogate_pair_is_split() {
    let mut state = MbState::INITIAL;
    assert_eq!(
        mbrtoc16(&mut state, MbMode::Utf8, &[0xf0, 0x9f, 0x98, 0x80]),
        Mbr16::Char {
            unit: 0xd83d,
            used: 4
        }
    );
    assert_eq!(
        mbrtoc16(&mut state, MbMode::Utf8, &[]),
        Mbr16::PendingLow { unit: 0xde00 }
    );
    assert!(state.is_initial());
}

#[test]
fn low_surrogate_is_not_produced_by_decoding() {
    let mut state = MbState::INITIAL;
    assert_eq!(
        mbrtoc16(&mut state, MbMode::Utf8, &[0xed, 0xa0, 0x80]),
        Mbr16::Ilseq
    );
}

#[test]
fn empty_input_is_incomplete_without_state_change() {
    let mut state = MbState {
        opaque1: 0xc0000000,
        opaque2: 0,
    };
    assert_eq!(mbrtoc16(&mut state, MbMode::Utf8, &[]), Mbr16::Incomplete);
    assert_eq!(state.opaque1, 0xc0000000);
}
