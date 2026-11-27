// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Unit tests for the raw constants and extended thread-create payload.

use super::*;
use crate::types::Handle;

#[test]
fn numbers_are_unique_and_roundtrip() {
    for (i, a) in Syscall::ALL.iter().enumerate() {
        assert_eq!(Syscall::from_usize(*a as usize), Some(*a));
        for b in &Syscall::ALL[i + 1..] {
            assert_ne!(*a as usize, *b as usize, "{a:?} and {b:?} share a number");
        }
    }
}

#[test]
fn unknown_numbers_are_none() {
    assert_eq!(Syscall::from_usize(0), None);
    assert_eq!(Syscall::from_usize(0xdead), None);
}

#[test]
fn m1_numbers_are_frozen() {
    assert_eq!(Syscall::EXIT as usize, 0x01);
    assert_eq!(Syscall::DEBUG_WRITE as usize, 0x03);
    assert_eq!(Syscall::UNAME as usize, 0x06);
    assert_eq!(Syscall::FUTEX as usize, 0x22);
    assert_eq!(Syscall::PROC_CREATE as usize, 0x30);
    assert_eq!(Syscall::PROC_WAIT as usize, 0x31);
    assert_eq!(Syscall::MAP as usize, 0x40);
    assert_eq!(Syscall::MEMOBJ_CREATE as usize, 0x43);
    assert_eq!(Syscall::HANDLE_DUP as usize, 0x51);
}

#[test]
fn handle_none_is_all_ones() {
    assert_eq!(Handle::NONE.0, u32::MAX);
}

#[test]
fn thread_extensions_are_frozen() {
    assert_eq!(THREAD_SELF, 0x24);
    assert_eq!(THREAD_EXIT_EX, 0x25);
    assert_eq!(ROBUST_LIST_SET, 0x26);
    assert_eq!(HANDLE_COPY, 0x54);
    assert_eq!(HANDLE_TRANSFER_FILE, 0x55);
    assert_eq!(THREAD_CREATE_EX, 1);
    assert_eq!(core::mem::size_of::<ThreadCreateEx>(), 24);
    let extended = ThreadCreateEx {
        tp: 0x1000,
        tid_out: 0x2000,
        clear_tid_addr: 0x3000,
    };
    assert_eq!(extended.tp, 0x1000);
    assert_eq!(extended.tid_out, 0x2000);
    assert_eq!(extended.clear_tid_addr, 0x3000);
}

#[test]
fn signal_syscalls_are_frozen() {
    assert_eq!(THREAD_KICK, 0x81);
    assert_eq!(PROC_SIGRECV_EX, 0x85);
    assert_eq!(PROC_SIGQUEUE, 0x86);
    assert_eq!(PROC_SIGRECV_DELIVERABLE, 2);
}

#[test]
fn wait6_raw_seam_is_frozen() {
    // Contract C21: the raw `PROC_WAIT6` number and the six-argument FreeBSD order.
    assert_eq!(PROC_WAIT6, 0x8E);
}
