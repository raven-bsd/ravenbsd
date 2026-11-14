// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

use super::*;
use crate::host_ut::HostSigKernel;

#[test]
fn tcb_words_round_trip_through_the_view() {
    let kernel = HostSigKernel::new();
    let tp = kernel.pointer();
    mask_store(tp, 0x55);
    assert_eq!(mask_load(tp), 0x55);
    assert_eq!(mask_fetch_or(tp, 0x2), 0x55);
    assert_eq!(mask_load(tp), 0x57);
    assert_eq!(mask_fetch_and(tp, !0x2), 0x57);
    assert_eq!(mask_swap(tp, 3), 0x55);
    assert_eq!(mask_load(tp), 3);
    pending_store(tp, 1);
    pending_or(tp, 4);
    assert_eq!(pending_load(tp), 5);
    pending_and(tp, !1);
    assert_eq!(pending_load(tp), 4);
}

#[test]
fn altstack_words_round_trip() {
    let kernel = HostSigKernel::new();
    let tp = kernel.pointer();
    altstack_set(tp, 0x1000, 0x2000, 1);
    assert_eq!(altstack_get(tp), (0x1000, 0x2000, 1));
}

#[test]
fn thread_handle_is_visible() {
    let kernel = HostSigKernel::new();
    assert_eq!(thread_handle(kernel.pointer()), crate::host_ut::FAKE_HANDLE);
}

#[test]
fn thread_state_view_fits_the_private_block() {
    assert!(core::mem::size_of::<SigThreadState>() <= 256);
    assert!(core::mem::align_of::<SigThreadState>() <= 8);
    let kernel = HostSigKernel::new();
    with_state(kernel.pointer(), |state| {
        *state = SigThreadState::new();
        state.add_critical();
        state.add_critical();
        state.add_defer();
        state.add_defer();
        state.add_defer();
    });
    let (critical, defer, _, _, _) = crate::critical::snapshot(kernel.pointer());
    assert_eq!(critical, 2);
    assert_eq!(defer, 3);
}

#[test]
fn pending_info_slot_round_trips() {
    let kernel = HostSigKernel::new();
    let mut info = SiginfoT::ZERO;
    info.si_signo = 9;
    info.si_code = 4;
    store_pending_info(kernel.pointer(), &info);
    assert_eq!(load_pending_info(kernel.pointer()).si_signo, 9);
}

#[test]
fn context_area_registration_and_copies() {
    let kernel = HostSigKernel::new();
    let tp = kernel.pointer();
    let mut machine = McontextT::ZERO;
    crate::frame::set_ctx_pc(&mut machine, 0xabc);
    crate::frame::set_ctx_sp(&mut machine, 0xdef);
    store_ctx(tp, &machine);
    let read = load_ctx(tp);
    assert_eq!(crate::frame::ctx_pc(&read), 0xabc);
    assert_eq!(crate::frame::ctx_sp(&read), 0xdef);
}

#[test]
fn stack_pointer_helpers_agree() {
    let first = current_stack_pointer();
    let second = stack_pointer_asm();
    assert!(first > 0);
    assert!(second > 0);
    assert!(first.abs_diff(second) < 1024);
}

#[test]
fn entry_and_start_addresses_are_symbols() {
    assert_ne!(sigentry_address(), 0);
    assert_ne!(start_context_address(), 0);
}

#[test]
fn mask_word_pointer_is_the_tcb_field() {
    let kernel = HostSigKernel::new();
    let pointer = mask_word_ptr(kernel.pointer());
    assert!(!pointer.is_null());
    let _ = pointer;
    mask_store(kernel.pointer(), 0x99);
    assert_eq!(mask_load(kernel.pointer()), 0x99);
}

#[test]
fn hooks_default_to_noop_and_can_be_installed() {
    set_cancel_hook(None);
    let mut pc = 5;
    assert!(!call_cancel_hook(&mut pc, 1));
    fn hook(pc: &mut usize, _bits: u32) -> bool {
        *pc = 42;
        true
    }
    set_cancel_hook(Some(hook));
    assert!(call_cancel_hook(&mut pc, 2));
    assert_eq!(pc, 42);
    set_cancel_hook(None);
    set_synccall_hook(None);
    call_synccall_hook();
    set_testcancel_hook(None);
    call_testcancel_hook();
    set_stop_hook(None);
    call_stop_hook(19);
    assert!(!call_exit_hook(0));
}

#[test]
fn entry_function_cell_round_trips() {
    let before = entry_function();
    set_entry_function(0x1234);
    assert_eq!(entry_function(), 0x1234);
    set_entry_function(before);
}

#[test]
fn call_handler_runs_on_a_separate_stack() {
    extern "C" fn triple(value: usize) -> usize {
        value.wrapping_mul(3)
    }
    let mut stack = vec![0u8; 16 * 1024];
    let top = stack.as_mut_ptr() as usize + 16 * 1024;
    let result = call_handler(top, triple as *const () as usize, 7, 0, 0);
    assert_eq!(result, 21);
}
