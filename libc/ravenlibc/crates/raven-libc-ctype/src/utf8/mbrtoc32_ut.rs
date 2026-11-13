// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
use super::*;

#[test]
fn decodes_like_mbrtowc() {
    let mut state = MbState::INITIAL;
    assert_eq!(
        mbrtoc32(&mut state, MbMode::Utf8, &[0x41]),
        Mbr::Char { wc: 0x41, used: 1 }
    );
    assert_eq!(
        mbrtoc32(&mut state, MbMode::Utf8, &[0xf0, 0x9f, 0x98, 0x80]),
        Mbr::Char {
            wc: 0x1f600,
            used: 4
        }
    );
    assert_eq!(mbrtoc32(&mut state, MbMode::Utf8, &[]), Mbr::Incomplete);
}
