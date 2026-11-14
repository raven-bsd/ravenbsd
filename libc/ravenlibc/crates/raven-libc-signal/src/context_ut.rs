// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

use super::*;
use crate::frame;
use raven_libc_abi::structs::{StackT, UcontextT};

fn stack(base: usize, size: usize) -> StackT {
    StackT {
        ss_sp: base as *mut core::ffi::c_void,
        ss_flags: 0,
        ss_size: size as u64,
    }
}

#[test]
fn makecontext_prepares_the_x86_register_image() {
    let buffer = vec![0u8; 32 * 1024];
    let base = buffer.as_ptr() as usize;
    let mut context = UcontextT::ZERO;
    frame::set_uc_stack(&mut context, &stack(base, 32 * 1024));
    makecontext_core(&mut context, 0x1234, &[1, 2, 3, 4, 5, 6]).unwrap();
    let machine = frame::mcontext_of(&context);
    assert_eq!(frame::ctx_pc(&machine), raw::start_context_address());
    assert_eq!(frame::ctx_sp(&machine), (base + 32 * 1024) & !15);
    assert_eq!(frame::ctx_greg(&machine, 8), 1);
    assert_eq!(frame::ctx_greg(&machine, 9), 2);
    assert_eq!(frame::ctx_greg(&machine, 12), 3);
    assert_eq!(frame::ctx_greg(&machine, 14), 4);
    assert_eq!(frame::ctx_greg(&machine, 0), 5);
    assert_eq!(frame::ctx_greg(&machine, 1), 6);
    #[cfg(target_arch = "x86_64")]
    {
        assert_eq!(frame::ctx_greg(&machine, 4), 0x1234);
    }
}

#[test]
fn too_many_arguments_are_e2big() {
    let buffer = vec![0u8; 16 * 1024];
    let mut context = UcontextT::ZERO;
    frame::set_uc_stack(&mut context, &stack(buffer.as_ptr() as usize, 16 * 1024));
    assert_eq!(
        makecontext_core(&mut context, 1, &[0, 1, 2, 3, 4, 5, 6]),
        Err(make_errno(errno::E2BIG))
    );
}

#[test]
fn a_missing_stack_is_einval() {
    let mut context = UcontextT::ZERO;
    assert_eq!(
        makecontext_core(&mut context, 1, &[]),
        Err(make_errno(errno::EINVAL))
    );
}

#[test]
fn uc_link_is_copied_into_the_prepared_image() {
    let buffer = vec![0u8; 16 * 1024];
    let mut context = UcontextT::ZERO;
    frame::set_uc_stack(&mut context, &stack(buffer.as_ptr() as usize, 16 * 1024));
    frame::set_uc_link(&mut context, 0x7777);
    makecontext_core(&mut context, 0x10, &[9, 8, 7]).unwrap();
    #[cfg(target_arch = "x86_64")]
    {
        let machine = frame::mcontext_of(&context);
        assert_eq!(frame::ctx_greg(&machine, 5), 0x7777);
    }
    assert_eq!(frame::uc_link(&context), 0x7777);
}
