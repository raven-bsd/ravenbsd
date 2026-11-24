// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! C19 frame layout and placement tests (x86_64).
//!
//! The values are the System V amd64 offsets printed by the repo headers
//! (`libc/ravenlibc/include/signal.h`, `arch/x86_64/include/bits/signal.h`):
//! `sigset_t` 128, `struct sigaction` 152, `siginfo_t` 128, `stack_t` 24,
//! `mcontext_t` 256, `ucontext_t` 936.

use super::*;

#[test]
fn sigframe_matches_the_system_v_amd64_offsets() {
    assert_eq!(core::mem::size_of::<UserStack>(), 24);
    assert_eq!(core::mem::offset_of!(UserStack, ss_flags), 8);
    assert_eq!(core::mem::offset_of!(UserStack, ss_size), 16);
    assert_eq!(core::mem::size_of::<UserUContext>(), 936);
    assert_eq!(core::mem::offset_of!(UserUContext, uc_stack), 16);
    assert_eq!(core::mem::offset_of!(UserUContext, uc_mcontext), 40);
    assert_eq!(core::mem::offset_of!(UserUContext, uc_sigmask), 296);
    assert_eq!(core::mem::size_of::<UserSigInfo>(), 128);
    assert_eq!(core::mem::size_of::<SigFrame>(), 1072);
    assert_eq!(core::mem::offset_of!(SigFrame, sf_si), 8);
    assert_eq!(core::mem::offset_of!(SigFrame, sf_uc), 136);
}

#[test]
fn frame_address_stays_below_the_red_zone() {
    let stack_pointer = 0x0000_7FFF_FFFF_0000u64;
    let frame = frame_address(stack_pointer).expect("frame fits");
    assert_eq!(frame & 0xf, 8, "SysV entry expects RSP % 16 == 8");
    assert!(
        frame + core::mem::size_of::<SigFrame>() as u64 <= stack_pointer - 128,
        "the frame must not overlap the 128-byte red zone"
    );
    assert!(is_user_address(frame));
}

#[test]
fn frame_address_rejects_underflow_and_the_kernel_half() {
    assert!(frame_address(0).is_none());
    assert!(frame_address(0x1000).is_none());
    assert!(frame_address(0xFFFF_8000_0000_0000).is_none());
}

#[test]
fn sigcode_trampoline_is_the_sigreturn_syscall() {
    assert_eq!(crate::process::SIGCODE[0], 0xB8);
    assert_eq!(crate::process::SIGCODE[1], signal::SIGRETURN as u8);
    assert_eq!(crate::process::SIGCODE[5], 0x0F);
    assert_eq!(crate::process::SIGCODE[6], 0x05);
}

#[test]
fn signal_bit_is_one_based() {
    assert_eq!(crate::process::signal_bit(1), 1);
    assert_eq!(crate::process::signal_bit(9), 0x100);
    assert_eq!(crate::process::signal_bit(64), 1u64 << 63);
    assert_eq!(crate::process::signal_bit(0), 0);
    assert_eq!(crate::process::signal_bit(65), 0);
}
