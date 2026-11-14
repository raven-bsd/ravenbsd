// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

use super::*;
use crate::host_ut::HostSigKernel;
use raven_libc_abi::structs::StackT;

fn stack(sp: usize, size: u64) -> StackT {
    StackT {
        ss_sp: sp as *mut core::ffi::c_void,
        ss_flags: 0,
        ss_size: size,
    }
}

#[test]
fn set_and_query_round_trip() {
    let kernel = HostSigKernel::new();
    let buffer = vec![0u8; 16 * 1024];
    let base = buffer.as_ptr() as usize;
    let new = stack(base, 16 * 1024);
    sigaltstack(&kernel, Some(&new), None).unwrap();
    let mut old = StackT::ZERO;
    sigaltstack(&kernel, None, Some(&mut old)).unwrap();
    assert_eq!(old.ss_sp as usize, base);
    assert_eq!(old.ss_size, 16 * 1024);
    assert_eq!(old.ss_flags & SS_DISABLE, 0);
}

#[test]
fn too_small_is_enomem() {
    let kernel = HostSigKernel::new();
    let buffer = vec![0u8; 1024];
    let new = stack(buffer.as_ptr() as usize, 1024);
    assert_eq!(sigaltstack(&kernel, Some(&new), None), Err(ENOMEM_ERR));
}

#[test]
fn ss_onstack_input_is_einval() {
    let kernel = HostSigKernel::new();
    let mut new = stack(0x1000, 16 * 1024);
    new.ss_flags = SS_ONSTACK;
    assert_eq!(sigaltstack(&kernel, Some(&new), None), Err(EINVAL_ERR));
}

#[test]
fn disable_clears_the_region() {
    let kernel = HostSigKernel::new();
    let buffer = vec![0u8; 16 * 1024];
    let new = stack(buffer.as_ptr() as usize, 16 * 1024);
    sigaltstack(&kernel, Some(&new), None).unwrap();
    let disable = stack(0, 0);
    let mut disabled = disable;
    disabled.ss_flags = SS_DISABLE;
    sigaltstack(&kernel, Some(&disabled), None).unwrap();
    let mut old = StackT::ZERO;
    sigaltstack(&kernel, None, Some(&mut old)).unwrap();
    assert_eq!(old.ss_flags, SS_DISABLE);
    assert!(old.ss_sp.is_null());
    assert!(!on_altstack(&kernel, buffer.as_ptr() as usize));
}

#[test]
fn changing_the_stack_while_on_it_is_eperm() {
    let kernel = HostSigKernel::new();
    let sp = crate::context::current_stack_pointer();
    let inside = stack(sp.saturating_sub(4096), 16 * 1024);
    sigaltstack(&kernel, Some(&inside), None).unwrap();
    let other = stack(0x1000_0000, 16 * 1024);
    assert_eq!(sigaltstack(&kernel, Some(&other), None), Err(EPERM_ERR));
}

#[test]
fn on_altstack_compares_the_range() {
    let kernel = HostSigKernel::new();
    let buffer = vec![0u8; 16 * 1024];
    let base = buffer.as_ptr() as usize;
    sigaltstack(&kernel, Some(&stack(base, 16 * 1024)), None).unwrap();
    assert!(on_altstack(&kernel, base));
    assert!(on_altstack(&kernel, base + 100));
    assert!(!on_altstack(&kernel, base + 16 * 1024));
    assert!(!on_altstack(&kernel, base.wrapping_sub(1)));
}
