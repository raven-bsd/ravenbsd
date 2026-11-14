// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! Tests for the seam shapes (`seams.rs`).

use crate::seams::*;

#[test]
fn mutex_has_the_frozen_pthread_size() {
    assert_eq!(core::mem::size_of::<MutexT>(), 40);
    assert_eq!(core::mem::align_of::<MutexT>(), 8);
}

#[test]
fn cond_has_the_frozen_pthread_size() {
    assert_eq!(core::mem::size_of::<CondT>(), 48);
    assert_eq!(core::mem::align_of::<CondT>(), 8);
}

#[test]
fn rwlock_has_the_frozen_pthread_size() {
    assert_eq!(core::mem::size_of::<RwlockT>(), 56);
    assert_eq!(core::mem::align_of::<RwlockT>(), 8);
}

#[test]
fn sema_has_the_frozen_sem_size() {
    assert_eq!(core::mem::size_of::<SemaT>(), 32);
    assert_eq!(core::mem::align_of::<SemaT>(), 8);
}

#[test]
fn zero_values_are_zero() {
    assert_eq!(MutexT::ZERO.opaque, [0; 40]);
    assert_eq!(CondT::ZERO.opaque, [0; 48]);
    assert_eq!(RwlockT::ZERO.opaque, [0; 56]);
    assert_eq!(SemaT::ZERO.opaque, [0; 32]);
}

#[test]
fn evq_ctl_values_match_the_syscall_documentation() {
    assert_eq!(EvqCtl::Add as u32, 1);
    assert_eq!(EvqCtl::Mod as u32, 2);
    assert_eq!(EvqCtl::Del as u32, 3);
}

#[test]
fn fd_kinds_are_distinct() {
    assert_ne!(FdKind::Port, FdKind::Pipe);
    assert_ne!(FdKind::Port, FdKind::Unknown);
}

#[test]
fn stack_plan_is_copy_and_equatable() {
    let plan = StackPlan::SetSize(32_768);
    let copy = plan;
    assert_eq!(plan, copy);
}

#[test]
fn thread_attr_plan_holds_both_parts() {
    let plan = ThreadAttrPlan {
        detached: true,
        stack: StackPlan::Default,
    };
    assert!(plan.detached);
    assert_eq!(plan.stack, StackPlan::Default);
}

#[test]
fn sigset_is_the_frozen_128_bytes() {
    let set: SigSet = [0; 16];
    assert_eq!(core::mem::size_of_val(&set), 128);
}

#[test]
fn timespec_is_reexported_from_the_abi() {
    assert_eq!(core::mem::size_of::<Timespec>(), 16);
}

#[test]
fn errno_is_the_platform_errno() {
    let error: raven_libc_platform::Errno = Errno::EINVAL;
    assert_eq!(error, Errno::EINVAL);
}
