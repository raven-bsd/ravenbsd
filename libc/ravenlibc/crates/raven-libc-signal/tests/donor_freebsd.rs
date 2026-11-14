// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Donor-test ports: FreeBSD `lib/libc/tests/gen/{sigsetops_test,sig2str_test,
//! makecontext_test}.c` as Rust cases (expected values, no C copied).

#![allow(
    unused_assignments,
    clippy::undocumented_unsafe_blocks,
    clippy::explicit_auto_deref,
    clippy::never_loop,
    clippy::identity_op,
    clippy::unnecessary_cast
)]
mod common;

use raven_libc_abi::structs::{StackT, UcontextT};
use raven_libc_signal::frame;
use raven_libc_signal::sig2str::{SIG2STR_MAX, sig2str, str2sig};
use raven_libc_signal::sigset::{APP_MASK, SIGRTMAX, SIGRTMIN, SigSet};

#[test]
fn sigsetops_empty_fill_add_del_member() {
    let mut set = SigSet::empty();
    assert!(set.is_empty());
    set.fill();
    assert!(!set.is_empty());
    assert_eq!(set.app_word(), APP_MASK);
    set.del(1).unwrap();
    assert!(!set.is_member(1).unwrap());
    set.add(1).unwrap();
    assert!(set.is_member(1).unwrap());
    for sig in 32..=34 {
        assert!(set.add(sig).is_err());
        assert!(set.del(sig).is_err());
    }
}

#[test]
fn sigsetops_and_or() {
    let left = SigSet::singleton(1).or(&SigSet::singleton(2));
    let right = SigSet::singleton(2).or(&SigSet::singleton(3));
    let and = left.and(&right);
    assert!(!and.is_member(1).unwrap());
    assert!(and.is_member(2).unwrap());
    assert!(!and.is_member(3).unwrap());
    let or = left.or(&right);
    for sig in 1..=3 {
        assert!(or.is_member(sig).unwrap());
    }
}

#[test]
fn sig2str_all_classic_names() {
    let expected = [
        (1, "HUP"),
        (2, "INT"),
        (3, "QUIT"),
        (4, "ILL"),
        (5, "TRAP"),
        (6, "ABRT"),
        (7, "BUS"),
        (8, "FPE"),
        (9, "KILL"),
        (10, "USR1"),
        (11, "SEGV"),
        (12, "USR2"),
        (13, "PIPE"),
        (14, "ALRM"),
        (15, "TERM"),
        (16, "STKFLT"),
        (17, "CHLD"),
        (18, "CONT"),
        (19, "STOP"),
        (20, "TSTP"),
        (21, "TTIN"),
        (22, "TTOU"),
        (23, "URG"),
        (24, "XCPU"),
        (25, "XFSZ"),
        (26, "VTALRM"),
        (27, "PROF"),
        (28, "WINCH"),
        (29, "IO"),
        (30, "PWR"),
        (31, "SYS"),
    ];
    for (number, name) in expected {
        let mut buffer = [0u8; SIG2STR_MAX];
        let length = sig2str(number, &mut buffer).unwrap();
        assert_eq!(&buffer[..length], name.as_bytes());
        assert_eq!(str2sig(name.as_bytes()), Ok(number));
    }
}

#[test]
fn sig2str_realtime_forms() {
    for (number, name) in [
        (35, "RTMIN"),
        (36, "RTMIN+1"),
        (49, "RTMIN+14"),
        (50, "RTMAX-14"),
        (63, "RTMAX-1"),
        (64, "RTMAX"),
    ] {
        let mut buffer = [0u8; SIG2STR_MAX];
        let length = sig2str(number, &mut buffer).unwrap();
        assert_eq!(&buffer[..length], name.as_bytes());
        assert_eq!(str2sig(name.as_bytes()), Ok(number));
    }
    assert_eq!(SIGRTMIN, 35);
    assert_eq!(SIGRTMAX, 64);
}

#[test]
fn sig2str_rejects_invalid_numbers() {
    for number in [-1, 0, 32, 33, 34, 65, 100] {
        let mut buffer = [0u8; SIG2STR_MAX];
        assert!(sig2str(number, &mut buffer).is_err());
    }
    assert!(str2sig(b"BOGUS").is_err());
}

#[test]
fn sig2str_numeric_strings() {
    for number in 1..=64 {
        assert_eq!(str2sig(format!("{number}").as_bytes()), Ok(number));
    }
}

#[test]
fn makecontext_writes_the_expected_image() {
    let mut stack = vec![0u8; 32 * 1024];
    let base = stack.as_mut_ptr() as usize;
    let stack_t = StackT {
        ss_sp: stack.as_mut_ptr().cast::<core::ffi::c_void>(),
        ss_flags: 0,
        ss_size: stack.len() as u64,
    };
    let mut context = UcontextT::ZERO;
    frame::set_uc_stack(&mut context, &stack_t);
    frame::set_uc_link(&mut context, 0x1000);
    raven_libc_signal::makecontext_core(&mut context, 0x2000, &[1, 2, 3]).unwrap();
    let machine = frame::mcontext_of(&context);
    assert_eq!(
        frame::ctx_pc(&machine),
        raven_libc_signal::raw::start_context_address()
    );
    #[cfg(target_arch = "x86_64")]
    {
        assert_eq!(frame::ctx_greg(&machine, 15), (base + 32 * 1024) & !15);
        assert_eq!(frame::ctx_greg(&machine, 4), 0x2000);
        assert_eq!(frame::ctx_greg(&machine, 8), 1);
    }
}
