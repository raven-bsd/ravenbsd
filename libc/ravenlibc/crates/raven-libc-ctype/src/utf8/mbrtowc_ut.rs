// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
use super::*;

#[test]
fn empty_input_leaves_state_untouched() {
    let mut state = MbState {
        opaque1: 0xc0000000,
        opaque2: 7,
    };
    assert_eq!(mbrtowc(&mut state, MbMode::Utf8, &[]), Mbr::Incomplete);
    assert_eq!(state.opaque1, 0xc0000000);
    assert_eq!(state.opaque2, 7);
}

#[test]
fn ascii_and_nul() {
    let mut state = MbState::INITIAL;
    assert_eq!(
        mbrtowc(&mut state, MbMode::Utf8, b"A"),
        Mbr::Char { wc: 0x41, used: 1 }
    );
    assert_eq!(
        mbrtowc(&mut state, MbMode::Utf8, b"\0"),
        Mbr::Char { wc: 0, used: 1 }
    );
    assert!(state.is_initial());
}

#[test]
fn resumes_partial_sequences_and_rejects_bad_bytes() {
    let mut state = MbState::INITIAL;
    assert_eq!(mbrtowc(&mut state, MbMode::Utf8, &[0xe2]), Mbr::Incomplete);
    assert!(!state.is_initial());
    assert_eq!(
        mbrtowc(&mut state, MbMode::Utf8, &[0x82, 0xac]),
        Mbr::Char {
            wc: 0x20ac,
            used: 2
        }
    );
    assert!(state.is_initial());
    assert_eq!(mbrtowc(&mut state, MbMode::Utf8, &[0xe0, 0x80]), Mbr::Ilseq);
    assert!(state.is_initial());
    assert_eq!(mbrtowc(&mut state, MbMode::Utf8, &[0xff]), Mbr::Ilseq);
}

#[test]
fn byte_mode_is_transparent() {
    let mut state = MbState::INITIAL;
    assert_eq!(
        mbrtowc(&mut state, MbMode::Byte, &[0xe9]),
        Mbr::Char {
            wc: 0xdfe9,
            used: 1
        }
    );
    assert_eq!(mbrtowc(&mut state, MbMode::Utf8, &[0xe9]), Mbr::Incomplete);
}

#[test]
fn reset_contract() {
    let mut state = MbState::INITIAL;
    assert_eq!(mbrtowc_reset(&mut state), Ok(()));
    state.opaque2 = 1;
    assert_eq!(mbrtowc_reset(&mut state), Err(MbError::Ilseq));
    assert!(state.is_initial());
}
