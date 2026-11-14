// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

use super::*;
use raven_libc_abi::structs::Sigval;

#[test]
fn the_layout_sizes_are_the_c36_contract() {
    assert_eq!(core::mem::size_of::<McontextT>(), SIZE_OF_MCONTEXT);
    assert_eq!(core::mem::size_of::<UcontextT>(), SIZE_OF_UCONTEXT);
}

#[cfg(target_arch = "x86_64")]
const SIZE_OF_MCONTEXT: usize = 256;
#[cfg(target_arch = "x86_64")]
const SIZE_OF_UCONTEXT: usize = 936;
#[cfg(target_arch = "aarch64")]
const SIZE_OF_MCONTEXT: usize = 4384;
#[cfg(target_arch = "aarch64")]
const SIZE_OF_UCONTEXT: usize = 4560;
#[cfg(target_arch = "riscv64")]
const SIZE_OF_MCONTEXT: usize = 784;
#[cfg(target_arch = "riscv64")]
const SIZE_OF_UCONTEXT: usize = 960;

#[test]
fn pc_sp_and_link_round_trip() {
    let mut machine = McontextT::ZERO;
    set_ctx_pc(&mut machine, 0x1234);
    set_ctx_sp(&mut machine, 0x5678);
    set_ctx_lr(&mut machine, 0x9abc);
    assert_eq!(ctx_pc(&machine), 0x1234);
    assert_eq!(ctx_sp(&machine), 0x5678);
    #[cfg(any(target_arch = "aarch64", target_arch = "riscv64"))]
    assert_eq!(ctx_lr(&machine), 0x9abc);
    #[cfg(target_arch = "x86_64")]
    assert_eq!(ctx_lr(&machine), 0);
}

#[test]
fn siginfo_fields_share_the_union_words() {
    let mut info = make_siginfo(10, 0);
    set_si_pid(&mut info, 42);
    set_si_uid(&mut info, 1000);
    assert_eq!(si_pid(&info), 42);
    assert_eq!(si_uid(&info), 1000);
    assert_eq!(si_field(&info, 0) & 0xffff_ffff, 42);
    assert_eq!((si_field(&info, 0) >> 32) as u32, 1000);
    set_si_value(&mut info, Sigval { bytes: [7] });
    assert_eq!(si_value(&info).bytes[0], 7);
    set_si_addr(&mut info, 0xdead);
    assert_eq!(si_addr(&info), 0xdead);
}

#[test]
fn user_and_queue_records_have_the_right_codes() {
    let user = user_siginfo(15, 0);
    assert_eq!(si_signo(&user), 15);
    assert_eq!(si_code(&user), 0);
    let queued = queue_siginfo(36, Sigval { bytes: [9] });
    assert_eq!(si_code(&queued), -1);
    assert_eq!(si_value(&queued).bytes[0], 9);
}

#[test]
fn ucontext_carries_the_mask_and_stack() {
    let machine = McontextT::ZERO;
    let stack = StackT {
        ss_sp: 0x1000 as *mut core::ffi::c_void,
        ss_flags: 0,
        ss_size: 0x2000,
    };
    let mut context = build_ucontext(&machine, 0x55, &stack);
    assert_eq!(uc_sigmask_word(&context), 0x55);
    assert_eq!(uc_stack_sp(&context), 0x1000);
    assert_eq!(uc_stack(&context).ss_size, 0x2000);
    set_uc_sigmask_word(&mut context, 0x66);
    assert_eq!(uc_sigmask_word(&context), 0x66);
    set_uc_link(&mut context, 0x777);
    assert_eq!(uc_link(&context), 0x777);
}

#[test]
fn mcontext_round_trips_through_a_ucontext() {
    let mut machine = McontextT::ZERO;
    set_ctx_pc(&mut machine, 0xabc);
    set_ctx_sp(&mut machine, 0xdef);
    set_ctx_greg(&mut machine, 3, 0x99);
    let mut context = UcontextT::ZERO;
    crate::raw::copy_mcontext_in(&machine, &mut context);
    let back = mcontext_of(&context);
    assert_eq!(ctx_pc(&back), 0xabc);
    assert_eq!(ctx_sp(&back), 0xdef);
    assert_eq!(ctx_greg(&back, 3), 0x99);
}

#[test]
fn context_pointer_helpers_are_inside_the_object() {
    let context = UcontextT::ZERO;
    let pointer = uc_mcontext_ptr(&context) as usize;
    let base = context.bytes.as_ptr() as usize;
    assert!(pointer >= base);
    assert!(pointer < base + core::mem::size_of::<UcontextT>());
}
