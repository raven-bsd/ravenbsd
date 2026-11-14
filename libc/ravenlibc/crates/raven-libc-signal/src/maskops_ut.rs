// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

use super::*;
use crate::host_ut::HostSigKernel;
use crate::mask;
use raven_libc_abi::generated::signal;

#[test]
fn block_unblock_and_setmask() {
    let kernel = HostSigKernel::new();
    mask::set_mask(&kernel, 0);
    let mut set = SigSet::empty();
    set.add(10).unwrap();
    let mut old = SigSet::empty();
    sigprocmask(&kernel, signal::SIG_BLOCK, Some(&set), Some(&mut old)).unwrap();
    assert_eq!(old.app_word(), 0);
    assert!(mask::is_blocked(mask::current_mask(&kernel), 10));
    sigprocmask(&kernel, signal::SIG_UNBLOCK, Some(&set), Some(&mut old)).unwrap();
    assert!(old.is_member(10).unwrap());
    assert!(!mask::is_blocked(mask::current_mask(&kernel), 10));
}

#[test]
fn setmask_replaces_and_clears_internal_bits() {
    let kernel = HostSigKernel::new();
    let mut set = SigSet::empty();
    if let Some(word) = set.0.first_mut() {
        *word = u64::MAX;
    }
    sigprocmask(&kernel, signal::SIG_SETMASK, Some(&set), None).unwrap();
    assert_eq!(mask::current_mask(&kernel), APP_MASK);
}

#[test]
fn bad_how_is_einval() {
    let kernel = HostSigKernel::new();
    assert_eq!(sigprocmask(&kernel, 7, None, None), Err(EINVAL_ERR));
    assert_eq!(pthread_sigmask(&kernel, 7, None, None), 22);
}

#[test]
fn null_set_is_a_query() {
    let kernel = HostSigKernel::new();
    mask::set_mask(&kernel, 0x40);
    let mut old = SigSet::empty();
    sigprocmask(&kernel, signal::SIG_BLOCK, None, Some(&mut old)).unwrap();
    assert!(old.is_member(7).unwrap());
    assert_eq!(mask::current_mask(&kernel), 0x40);
}

#[test]
fn unblocking_a_pending_signal_self_kicks() {
    let kernel = HostSigKernel::new();
    mask::set_mask(&kernel, crate::critical::signal_bit(12));
    crate::raw::pending_or(kernel.pointer(), crate::critical::signal_bit(12));
    kernel.clear_trace();
    let mut set = SigSet::empty();
    set.add(12).unwrap();
    sigprocmask(&kernel, signal::SIG_UNBLOCK, Some(&set), None).unwrap();
    assert!(kernel.kick_word() & crate::kernel::KICK_CHECK != 0);
}

#[test]
fn sigpending_reports_thread_bits() {
    let kernel = HostSigKernel::new();
    crate::raw::pending_store(kernel.pointer(), 0x5);
    let mut pending = SigSet::empty();
    sigpending(&kernel, &mut pending).unwrap();
    assert!(pending.is_member(1).unwrap());
    assert!(pending.is_member(3).unwrap());
    assert!(!pending.is_member(2).unwrap());
}
