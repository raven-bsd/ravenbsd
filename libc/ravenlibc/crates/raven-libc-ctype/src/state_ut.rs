// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
use super::*;

#[test]
fn size_and_alignment_match_the_c_abi() {
    assert_eq!(core::mem::size_of::<MbState>(), 8);
    assert_eq!(core::mem::align_of::<MbState>(), 4);
    assert_eq!(
        core::mem::size_of::<MbState>(),
        core::mem::size_of::<raven_libc_abi::generated::structs::MbstateT>()
    );
}

#[test]
fn initial_state_is_both_words_zero() {
    assert!(MbState::INITIAL.is_initial());
    assert_eq!(MbState::default(), MbState::INITIAL);
    assert!(
        !MbState {
            opaque1: 1,
            opaque2: 0
        }
        .is_initial()
    );
    assert!(
        !MbState {
            opaque1: 0,
            opaque2: 2
        }
        .is_initial()
    );
}
