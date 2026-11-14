// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

use super::*;
use raven_libc_abi::structs::JmpBuf;

#[test]
fn tail_saves_on_zero_and_restores_on_nonzero() {
    let mut buffer = JmpBuf::ZERO;
    let mut mask = 0x1234u64;
    assert_eq!(sigsetjmp_tail(&mut buffer, 0, &mut mask), 0);
    assert_eq!(buffer.bytes[raw::JMP_SS_OFFSET >> 3], 0x1234);
    let mut live = 0u64;
    assert_eq!(sigsetjmp_tail(&mut buffer, 1, &mut live), 1);
    assert_eq!(live, 0x1234);
}

#[test]
fn tail_returns_the_jump_value() {
    let mut buffer = JmpBuf::ZERO;
    let mut mask = 7u64;
    for value in [2, 3, 99, -1, i32::MAX] {
        assert_eq!(sigsetjmp_tail(&mut buffer, value, &mut mask), value);
    }
}

#[test]
fn jmp_offsets_are_the_rebased_ones() {
    assert_eq!(raw::JMP_FL_OFFSET, 256);
    assert_eq!(raw::JMP_SS_OFFSET, 264);
    assert_eq!(raw::JMP_SS_SCRATCH, 272);
    assert_eq!(raw::JMP_SS_MASK_PTR, 280);
    assert_eq!(core::mem::size_of::<JmpBuf>(), 392);
}

#[test]
fn constants_are_zero_and_one() {
    assert_eq!(SETJMP, 0);
    assert_eq!(LONGJMP, 1);
}
