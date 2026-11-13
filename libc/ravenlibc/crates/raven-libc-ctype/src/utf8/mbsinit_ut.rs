// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
use super::*;

#[test]
fn mbsinit_needs_both_words_zero() {
    assert!(mbsinit(&MbState::INITIAL));
    assert!(!mbsinit(&MbState {
        opaque1: 1,
        opaque2: 0
    }));
    assert!(!mbsinit(&MbState {
        opaque1: 0,
        opaque2: 1
    }));
}
