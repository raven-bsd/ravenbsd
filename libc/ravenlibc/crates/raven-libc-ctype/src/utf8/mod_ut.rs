// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
use super::*;

#[test]
fn mbr_c_return_matches_musl() {
    assert_eq!(Mbr::Char { wc: 0, used: 1 }.c_return(), 0);
    assert_eq!(Mbr::Char { wc: 0x41, used: 1 }.c_return(), 1);
    assert_eq!(Mbr::Char { wc: 0x100, used: 2 }.c_return(), 2);
    assert_eq!(Mbr::Incomplete.c_return(), -2);
    assert_eq!(Mbr::Ilseq.c_return(), -1);
}

#[test]
fn mbr16_and_mbr8_pending_returns_are_minus_three() {
    assert_eq!(Mbr16::PendingLow { unit: 0xdca9 }.c_return(), -3);
    assert_eq!(Mbr16::Char { unit: 0, used: 1 }.c_return(), 0);
    assert_eq!(Mbr8::Continuation { unit: 0x9f }.c_return(), -3);
    assert_eq!(Mbr8::Char { unit: 0, used: 1 }.c_return(), 0);
    assert_eq!(
        Mbr8::Char {
            unit: 0xf0,
            used: 4
        }
        .c_return(),
        4
    );
}

#[test]
fn strconv_carries_the_source_position_and_state() {
    let conv = StrConv {
        ret: Ok(3),
        src: SrcPos::Nul,
        state: MbState::INITIAL,
    };
    assert_eq!(conv.ret, Ok(3));
    assert_eq!(conv.src, SrcPos::Nul);
    assert!(conv.state.is_initial());
    assert_ne!(SrcPos::Nul, SrcPos::At(0));
}
