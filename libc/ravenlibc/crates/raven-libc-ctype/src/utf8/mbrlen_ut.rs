// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
use super::*;
use crate::utf8::mbsinit;

#[test]
fn mbrlen_tracks_the_shared_state() {
    let mut state = MbState::INITIAL;
    assert_eq!(mbrlen(&mut state, MbMode::Utf8, &[0xe2]), Mbr::Incomplete);
    assert_eq!(
        mbrlen(&mut state, MbMode::Utf8, &[0x82, 0xac]),
        Mbr::Char {
            wc: 0x20ac,
            used: 2
        }
    );
    assert!(mbsinit(&state));
}
