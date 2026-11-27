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
fn reset_after_fork_clears_delivery_state_and_keeps_the_mask() {
    let kernel = HostSigKernel::new();
    let tp = kernel.pointer();
    mask_store(tp, 0x55);
    pending_store(tp, 9);
    let mut info = SiginfoT::ZERO;
    info.si_signo = 17;
    store_pending_info(tp, &info);

    assert!(!reset_after_fork(tp), "C19 has no entry to re-register");

    assert_eq!(mask_load(tp), 0x55, "the child inherits the signal mask");
    assert_eq!(pending_load(tp), 0);
    assert_eq!(load_pending_info(tp).si_signo, 0);
    assert!(!reset_after_fork(tp), "a second reset is still entry-free");
}

#[test]
fn the_user_entry_registration_is_deleted() {
    use std::path::Path;

    fn source_files(dir: &Path, out: &mut Vec<std::path::PathBuf>) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                source_files(&path, out);
            } else if path.extension().is_some_and(|ext| ext == "rs") {
                out.push(path);
            }
        }
    }

    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut files = Vec::new();
    source_files(&src, &mut files);
    let trampoline = ["__raven_", "sigentry"].concat();
    let address = ["fn ", "sigentry_address"].concat();
    let setup = ["fn ", "sig_setup"].concat();
    let mut violations = Vec::new();
    for file in files {
        let text = std::fs::read_to_string(&file).expect("read a signal source file");
        for needle in [&trampoline, &address, &setup] {
            if text.contains(needle) {
                violations.push(format!("{}: {needle}", file.display()));
            }
        }
    }
    for name in ["entry_x86_64.rs", "entry_aarch64.rs", "entry_riscv64.rs"] {
        let path = src.join("raw").join(name);
        if path.exists() {
            violations.push(format!("{}: entry trampoline file", path.display()));
        }
    }
    assert!(
        violations.is_empty(),
        "the C19 user entry must be deleted: {violations:?}"
    );
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
fn start_address_is_a_symbol() {
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
fn call_handler_runs_on_a_separate_stack() {
    extern "C" fn triple(value: usize) -> usize {
        value.wrapping_mul(3)
    }
    let mut stack = vec![0u8; 16 * 1024];
    let top = stack.as_mut_ptr() as usize + 16 * 1024;
    let result = call_handler(top, triple as *const () as usize, 7, 0, 0);
    assert_eq!(result, 21);
}

// --- K443: the context window is no longer tied to the builtin TLS page ---

#[test]
fn builtin_tail_is_selected_for_a_builtin_tls_pointer() {
    let base = raven_libc_tls::raw::BUILTIN_TLS.as_ptr() as usize;
    let end = base + raven_libc_tls::raw::BUILTIN_TLS.capacity();
    let needed =
        core::mem::size_of::<McontextT>() + core::mem::size_of::<crate::kernel::FaultRecord>();
    let (context, offset) = builtin_context(base, base, end, needed).expect("builtin tail");
    assert_eq!(offset, context - base);
    assert!(context >= base);
    assert!(context + needed <= end);
}

#[test]
fn a_foreign_tls_pointer_has_no_builtin_tail() {
    let base = raven_libc_tls::raw::BUILTIN_TLS.as_ptr() as usize;
    let end = base + raven_libc_tls::raw::BUILTIN_TLS.capacity();
    let needed =
        core::mem::size_of::<McontextT>() + core::mem::size_of::<crate::kernel::FaultRecord>();
    let local = 0u8;
    let foreign = (&local as *const u8 as usize) & !15;
    assert_eq!(builtin_context(foreign, base, end, needed), None);
}

#[test]
fn context_area_argument_and_flags_describe_the_registration() {
    let relative = ContextArea::Relative {
        context: 0x1000,
        offset: 0x120,
    };
    assert_eq!(relative.context(), 0x1000);
    assert_eq!(relative.argument(), 0x120);
    assert_eq!(relative.flags(), 0);
    let absolute = ContextArea::Absolute { context: 0x9000 };
    assert_eq!(absolute.context(), 0x9000);
    assert_eq!(absolute.argument(), 0x9000);
    assert_eq!(absolute.flags(), SIG_SETUP_ABS);
}
