// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
use super::*;

#[test]
fn encodes_a_four_byte_character_after_four_units() {
    let mut state = MbState::INITIAL;
    let mut out = [0xccu8; 4];
    assert_eq!(c8rtomb(&mut state, MbMode::Utf8, 0xf0, &mut out), Ok(0));
    assert_eq!(c8rtomb(&mut state, MbMode::Utf8, 0x9f, &mut out), Ok(0));
    assert_eq!(c8rtomb(&mut state, MbMode::Utf8, 0x92, &mut out), Ok(0));
    assert_eq!(c8rtomb(&mut state, MbMode::Utf8, 0xa9, &mut out), Ok(4));
    assert_eq!(out, [0xf0, 0x9f, 0x92, 0xa9]);
    assert!(state.is_initial());
}

#[test]
fn invalid_continuation_is_ilseq_and_writes_nothing() {
    let mut state = MbState::INITIAL;
    let mut out = [0xccu8; 4];
    assert_eq!(c8rtomb(&mut state, MbMode::Utf8, 0xf0, &mut out), Ok(0));
    assert_eq!(c8rtomb(&mut state, MbMode::Utf8, 0x9f, &mut out), Ok(0));
    assert_eq!(c8rtomb(&mut state, MbMode::Utf8, 0x92, &mut out), Ok(0));
    assert_eq!(
        c8rtomb(&mut state, MbMode::Utf8, 0x41, &mut out),
        Err(MbError::Ilseq)
    );
    assert_eq!(out[0], 0xcc);
}

#[test]
fn nul_discards_pending_input_and_writes_nul() {
    let mut state = MbState::INITIAL;
    let mut out = [0xccu8; 4];
    assert_eq!(c8rtomb(&mut state, MbMode::Utf8, 0xf0, &mut out), Ok(0));
    assert_eq!(c8rtomb(&mut state, MbMode::Utf8, 0, &mut out), Ok(1));
    assert_eq!(out[0], 0);
    assert_eq!(out[1], 0xcc);
}
