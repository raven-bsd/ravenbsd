// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
use super::*;

#[test]
fn high_surrogate_stores_state_and_low_completes() {
    let mut state = MbState::INITIAL;
    let mut out = [0u8; 4];
    assert_eq!(c16rtomb(&mut state, MbMode::Utf8, 0xd83d, &mut out), Ok(0));
    assert_eq!(state.opaque1, 0x1f400);
    assert_eq!(c16rtomb(&mut state, MbMode::Utf8, 0xde00, &mut out), Ok(4));
    assert_eq!(out, [0xf0, 0x9f, 0x98, 0x80]);
    assert!(state.is_initial());
}

#[test]
fn lone_low_surrogate_is_ilseq() {
    let mut state = MbState::INITIAL;
    let mut out = [0u8; 4];
    assert_eq!(
        c16rtomb(&mut state, MbMode::Utf8, 0xdc00, &mut out),
        Err(MbError::Ilseq)
    );
    assert_eq!(c16rtomb(&mut state, MbMode::Utf8, 0xd800, &mut out), Ok(0));
    assert_eq!(
        c16rtomb(&mut state, MbMode::Utf8, 0x41, &mut out),
        Err(MbError::Ilseq)
    );
    assert!(state.is_initial());
}
