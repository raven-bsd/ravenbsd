// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! x86-64 signal-frame layout and round-trip tests.

use core::mem::{align_of, offset_of, size_of};

use linux_shim::sigframe::{
    AltStack, FRAME_BYTES, MCONTEXT_AT, Regs, SA_ONSTACK, SA_RESTORER, SIGINFO_AT, SS_DISABLE,
    SigInfo, UCONTEXT_AT, UCONTEXT_MASK_AT, build_rt_sigframe, restore_rt_sigframe,
};
use raven_linux_abi::structs::x86_64::SigAction;

const USER_SP: usize = 0x7fff_0000;
const RESTORER: usize = 0x401000;
const HANDLER: usize = 0x402000;

fn action(flags: u64) -> SigAction {
    SigAction {
        sa_handler: HANDLER,
        sa_flags: SA_RESTORER | flags,
        sa_restorer: RESTORER,
        sa_mask: 0,
    }
}

fn info() -> SigInfo {
    let mut fields = [0u8; 112];
    fields[..4].copy_from_slice(&0xdead_beefu32.to_le_bytes());
    SigInfo::new(11, 0, 1, fields)
}

#[test]
fn frame_offsets_match_the_documented_layout() {
    assert_eq!(
        size_of::<SigInfo>(),
        size_of::<linux_raw_sys::general::siginfo>()
    );
    assert_eq!(
        align_of::<SigInfo>(),
        align_of::<linux_raw_sys::general::siginfo>()
    );
    assert_eq!(
        size_of::<SigAction>(),
        size_of::<linux_raw_sys::general::sigaction>()
    );
    assert_eq!(offset_of!(linux_raw_sys::general::sigaltstack, ss_sp), 0);
    assert_eq!(offset_of!(linux_raw_sys::general::sigaltstack, ss_flags), 8);
    assert_eq!(offset_of!(linux_raw_sys::general::sigaltstack, ss_size), 16);
    assert_eq!(
        UCONTEXT_AT + 16 + offset_of!(linux_raw_sys::general::sigaltstack, ss_sp),
        24
    );
    assert_eq!(
        UCONTEXT_AT + 16 + offset_of!(linux_raw_sys::general::sigaltstack, ss_flags),
        32
    );
    assert_eq!(
        UCONTEXT_AT + 16 + offset_of!(linux_raw_sys::general::sigaltstack, ss_size),
        40
    );
    assert_eq!(FRAME_BYTES, 8 + 304 + 128);
    assert_eq!(SIGINFO_AT, 312);
    assert_eq!(MCONTEXT_AT, 48);
    assert_eq!(UCONTEXT_MASK_AT, 304);
}

#[test]
fn build_sets_the_restorer_and_handler() {
    let regs = Regs::default();
    let frame = build_rt_sigframe(&regs, 11, &info(), &action(0), 0, None, USER_SP, 0);
    assert_eq!(frame.bytes.len(), FRAME_BYTES);
    assert_eq!(frame.handler_rip, HANDLER);
    assert_eq!(
        u64::from_le_bytes(frame.bytes[0..8].try_into().unwrap()),
        RESTORER as u64
    );
}

#[test]
fn alignment_and_red_zone_hold() {
    for fpstate_len in [0usize, 512, 1024] {
        let frame = build_rt_sigframe(
            &Regs::default(),
            11,
            &info(),
            &action(0),
            0,
            None,
            USER_SP,
            fpstate_len,
        );
        assert_eq!((frame.new_sp + 8) % 16, 0);
        assert!(frame.new_sp + FRAME_BYTES + fpstate_len <= USER_SP - 128);
    }
}

#[test]
fn fpstate_pointer_is_above_the_frame() {
    let frame = build_rt_sigframe(
        &Regs::default(),
        11,
        &info(),
        &action(0),
        0,
        None,
        USER_SP,
        512,
    );
    let fpstate = u64::from_le_bytes(
        frame.bytes[MCONTEXT_AT + 184..MCONTEXT_AT + 192]
            .try_into()
            .unwrap(),
    );
    assert_ne!(fpstate, 0);
    assert_eq!(fpstate % 64, 0);
    assert!(fpstate as usize >= frame.new_sp + FRAME_BYTES);
}

#[test]
fn no_fpstate_has_a_null_pointer() {
    let frame = build_rt_sigframe(
        &Regs::default(),
        11,
        &info(),
        &action(0),
        0,
        None,
        USER_SP,
        0,
    );
    assert_eq!(
        u64::from_le_bytes(
            frame.bytes[MCONTEXT_AT + 184..MCONTEXT_AT + 192]
                .try_into()
                .unwrap()
        ),
        0
    );
}

#[test]
fn altstack_is_selected_for_sa_onstack() {
    let altstack = AltStack {
        sp: 0x1000_0000,
        flags: 0,
        size: 0x4000,
    };
    let frame = build_rt_sigframe(
        &Regs::default(),
        11,
        &info(),
        &action(SA_ONSTACK),
        0,
        Some(altstack),
        0x2000_0000,
        0,
    );
    assert!(frame.new_sp >= altstack.sp);
    assert!(frame.new_sp + FRAME_BYTES <= altstack.sp + altstack.size);
}

#[test]
fn disabled_altstack_falls_back_to_the_user_stack() {
    let altstack = AltStack {
        sp: 0x1000_0000,
        flags: SS_DISABLE,
        size: 0x4000,
    };
    let frame = build_rt_sigframe(
        &Regs::default(),
        11,
        &info(),
        &action(SA_ONSTACK),
        0,
        Some(altstack),
        USER_SP,
        0,
    );
    assert!(frame.new_sp <= USER_SP - 128);
}

#[test]
fn missing_altstack_is_marked_disabled() {
    let frame = build_rt_sigframe(
        &Regs::default(),
        11,
        &info(),
        &action(0),
        0,
        None,
        USER_SP,
        0,
    );
    let flags = u32::from_le_bytes(frame.bytes[24 + 8..24 + 12].try_into().unwrap());
    assert_eq!(flags, SS_DISABLE as u32);
}

#[test]
fn siginfo_payload_is_written() {
    let frame = build_rt_sigframe(
        &Regs::default(),
        11,
        &info(),
        &action(0),
        0,
        None,
        USER_SP,
        0,
    );
    assert_eq!(
        i32::from_le_bytes(frame.bytes[SIGINFO_AT..SIGINFO_AT + 4].try_into().unwrap()),
        11
    );
    assert_eq!(
        u32::from_le_bytes(
            frame.bytes[SIGINFO_AT + 16..SIGINFO_AT + 20]
                .try_into()
                .unwrap()
        ),
        0xdead_beef
    );
}

#[test]
fn restore_rejects_a_short_frame() {
    assert!(restore_rt_sigframe(&[0u8; FRAME_BYTES - 1]).is_err());
}

#[test]
fn restore_round_trips_mask_and_registers() {
    let mask = 0x5a5a_5a5a_5a5a_5a5a;
    let regs = Regs {
        r8: 8,
        r15: 15,
        rdi: 0x1111,
        rsi: 0x2222,
        rsp: 0x3333,
        rip: 0x4444,
        rflags: 0x202,
        cs: 0x33,
        ss: 0x2b,
        oldmask: mask,
        ..Regs::default()
    };
    let frame = build_rt_sigframe(&regs, 11, &info(), &action(0), mask, None, USER_SP, 0);
    let (restored, restored_mask) = restore_rt_sigframe(&frame.bytes).unwrap();
    assert_eq!(restored, regs);
    assert_eq!(restored_mask, mask);
}

#[test]
fn thousand_random_register_sets_round_trip() {
    let mut seed = 0x1234_5678_9abc_def0u64;
    for _ in 0..1000 {
        let mut regs = Regs::default();
        for field in [
            &mut regs.r8,
            &mut regs.r9,
            &mut regs.r10,
            &mut regs.r11,
            &mut regs.r12,
            &mut regs.r13,
            &mut regs.r14,
            &mut regs.r15,
            &mut regs.rdi,
            &mut regs.rsi,
            &mut regs.rbp,
            &mut regs.rbx,
            &mut regs.rdx,
            &mut regs.rax,
            &mut regs.rcx,
            &mut regs.rsp,
            &mut regs.rip,
            &mut regs.rflags,
        ] {
            *field = next(&mut seed);
        }
        regs.cs = next(&mut seed) as u16;
        regs.gs = next(&mut seed) as u16;
        regs.fs = next(&mut seed) as u16;
        regs.ss = next(&mut seed) as u16;
        regs.err = next(&mut seed);
        regs.trapno = next(&mut seed);
        regs.cr2 = next(&mut seed);
        let mask = next(&mut seed);
        regs.oldmask = mask;
        let frame = build_rt_sigframe(
            &regs,
            next(&mut seed) as u32,
            &info(),
            &action(0),
            mask,
            None,
            USER_SP,
            0,
        );
        let (restored, restored_mask) = restore_rt_sigframe(&frame.bytes).unwrap();
        assert_eq!(restored, regs);
        assert_eq!(restored_mask, mask);
        assert_eq!((frame.new_sp + 8) % 16, 0);
    }
}

fn next(seed: &mut u64) -> u64 {
    let mut value = *seed;
    value ^= value << 13;
    value ^= value >> 7;
    value ^= value << 17;
    *seed = value;
    value
}
