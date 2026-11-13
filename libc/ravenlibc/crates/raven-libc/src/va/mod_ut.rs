// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! Unit tests of the module constants and class helpers (`mod.rs`).

use super::*;

#[test]
fn class_kinds_are_the_reader_operations() {
    assert_eq!(VaClass::Int.kind(), VaKind::Int);
    assert_eq!(VaClass::Long.kind(), VaKind::Long);
    assert_eq!(VaClass::Ptr.kind(), VaKind::Ptr);
    assert_eq!(VaClass::Double.kind(), VaKind::Double);
    assert_eq!(VaClass::LongDouble.kind(), VaKind::LongDouble);
    assert_eq!(VaClass::Agg { size: 12, align: 8 }.kind(), VaKind::Agg(12));
}

#[test]
fn alignment_helpers_round_up() {
    for (input, expected) in [(0, 0), (1, 16), (15, 16), (16, 16), (17, 32), (240, 240)] {
        assert_eq!(align_up_16(input), expected);
    }
    for (input, expected) in [(0, 0), (1, 8), (7, 8), (8, 8), (9, 16), (80, 80)] {
        assert_eq!(align_up_8(input), expected);
    }
}

#[test]
fn the_aggregate_subset_is_exactly_the_documented_one() {
    for size in [8usize, 12, 16, 24, 32] {
        assert!(aggregate_in_subset(size, 8));
    }
    for size in [0usize, 1, 4, 15, 17, 20, 31, 33, 64] {
        assert!(
            !aggregate_in_subset(size, 8),
            "size {size} must be rejected"
        );
    }
    for align in [1usize, 2, 4, 16, 32] {
        assert!(
            !aggregate_in_subset(8, align),
            "align {align} must be rejected"
        );
    }
}

#[test]
fn named_argument_bounds_match_the_abis() {
    // x86_64: six integer argument registers, the va_list needs one.
    assert_eq!(MAX_NAMED_X86_64, 5);
    // aarch64/riscv64: eight integer argument registers.
    assert_eq!(MAX_NAMED_OTHER, 7);
}

#[test]
fn frame_constants_are_the_contract() {
    assert_eq!(FRAME_X86_64, 208);
    assert_eq!(FRAME_X86_64 % 16, 0);
    assert_eq!(LIST_OFFSET_X86_64 + 24, 200);
    assert_eq!(FRAME_AAPCS64, 240);
    assert_eq!(LIST_OFFSET_AAPCS64 + 32, FRAME_AAPCS64);
    assert_eq!(FRAME_RISCV64, 80);
    assert_eq!(SAVE_OFFSET_RISCV64 + 64, FRAME_RISCV64);
}
