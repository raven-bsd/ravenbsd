// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Unit tests for the aarch64 interruptible syscall set.

use super::*;

#[test]
fn unmap_is_interruptible() {
    assert!(interruptible(Syscall::UNMAP as usize));
}

#[test]
fn map_and_protect_are_interruptible() {
    assert!(interruptible(Syscall::MAP as usize));
    assert!(interruptible(Syscall::PROTECT as usize));
}

#[test]
fn ipc_pread_is_interruptible() {
    assert!(interruptible(crate::syscall::ipc::PREAD));
}

#[test]
fn sigreturn_is_not_interruptible() {
    assert!(!interruptible(raven_abi::signal::SIGRETURN));
}

#[test]
fn unknown_number_is_not_interruptible() {
    assert!(!interruptible(usize::MAX));
}
