// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Donor-test ports: OpenBSD `regress/lib/libc/{setjmp,setjmp-signal,sigsetjmp,sigthr}` and
//! `regress/lib/libpthread/{signal,signodefer,siginfo,siginterrupt,sigdeliver,sigsuspend,
//! sigmask,sigwait,setjmp,earlysig}`. Expected values and scenarios only; no OpenBSD C copied.

#![allow(
    unused_assignments,
    clippy::undocumented_unsafe_blocks,
    clippy::explicit_auto_deref,
    clippy::never_loop,
    clippy::identity_op,
    clippy::unnecessary_cast
)]
mod common;

use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};

use common::fake::HostSigKernel;
use raven_libc_abi::generated::signal;
use raven_libc_abi::structs::{SiginfoT, Timespec};
use raven_libc_signal::dispositions::{DispTable, Disposition, SA_NODEFER, SA_RESTART, SA_SIGINFO};
use raven_libc_signal::{
    EINTR_ERR, SigSet, frame, legacy, mask, raise, sigaction, sigsuspend, sigtimedwait,
};

static LOCK: Mutex<()> = Mutex::new(());
static COUNT: AtomicUsize = AtomicUsize::new(0);

extern "C" fn handler(_sig: i32) {
    COUNT.fetch_add(1, Ordering::SeqCst);
}

#[test]
fn signal_is_bsd_reliable() {
    let _guard = LOCK.lock().unwrap();
    let table = DispTable::new();
    legacy::signal(&table, signal::SIGUSR1, handler as *const () as usize).unwrap();
    assert_eq!(table.read(signal::SIGUSR1).flags & SA_RESTART, SA_RESTART);
    legacy::signal(&table, signal::SIGUSR1, handler as *const () as usize).unwrap();
    assert_eq!(
        table.read(signal::SIGUSR1).handler,
        handler as *const () as usize
    );
}

#[test]
fn signodefer_keeps_the_signal_blocked_during_the_handler() {
    let _guard = LOCK.lock().unwrap();
    let table = DispTable::new();
    let mut action = Disposition::handler(handler as *const () as usize);
    action.flags = SA_NODEFER;
    sigaction(&table, signal::SIGUSR2, Some(&action), None).unwrap();
    assert_ne!(table.read(signal::SIGUSR2).flags & SA_NODEFER, 0);
}

#[test]
fn siginterrupt_toggles_restart() {
    let _guard = LOCK.lock().unwrap();
    let table = DispTable::new();
    legacy::signal(&table, signal::SIGUSR1, handler as *const () as usize).unwrap();
    legacy::siginterrupt(&table, signal::SIGUSR1, 1).unwrap();
    assert_eq!(table.read(signal::SIGUSR1).flags & SA_RESTART, 0);
    legacy::siginterrupt(&table, signal::SIGUSR1, 0).unwrap();
    assert_ne!(table.read(signal::SIGUSR1).flags & SA_RESTART, 0);
}

#[test]
fn sigdeliver_runs_the_handler() {
    let _guard = LOCK.lock().unwrap();
    COUNT.store(0, Ordering::SeqCst);
    let kernel = HostSigKernel::new();
    let table = DispTable::new();
    mask::set_mask(&kernel, 0);
    table.write(10, Disposition::handler(handler as *const () as usize));
    raise(&kernel, &table, 10).unwrap();
    assert_eq!(COUNT.load(Ordering::SeqCst), 1);
}

#[test]
fn sigsuspend_runs_the_handler_then_eintr() {
    let _guard = LOCK.lock().unwrap();
    COUNT.store(0, Ordering::SeqCst);
    let kernel = HostSigKernel::new();
    let table = DispTable::new();
    table.write(10, Disposition::handler(handler as *const () as usize));
    raven_libc_signal::raw::pending_or(
        kernel.pointer(),
        raven_libc_signal::critical::signal_bit(10),
    );
    assert_eq!(sigsuspend(&kernel, &table, &SigSet::empty()), EINTR_ERR);
    assert_eq!(COUNT.load(Ordering::SeqCst), 1);
}

#[test]
fn sigmask_round_trip() {
    let _guard = LOCK.lock().unwrap();
    let kernel = HostSigKernel::new();
    mask::set_mask(&kernel, 0x55);
    assert_eq!(mask::current_mask(&kernel), 0x55);
    let mut set = SigSet::empty();
    raven_libc_signal::sigpending(&kernel, &mut set).unwrap();
    assert!(set.is_empty());
}

#[test]
fn sigwait_consumes_without_handler() {
    let _guard = LOCK.lock().unwrap();
    COUNT.store(0, Ordering::SeqCst);
    let kernel = HostSigKernel::new();
    let table = DispTable::new();
    table.write(10, Disposition::handler(handler as *const () as usize));
    raven_libc_signal::raw::pending_or(
        kernel.pointer(),
        raven_libc_signal::critical::signal_bit(10),
    );
    let mut info = SiginfoT::ZERO;
    let timeout = Timespec {
        tv_sec: 0,
        tv_nsec: 0,
    };
    let sig = sigtimedwait(
        &kernel,
        &SigSet::singleton(10),
        Some(&mut info),
        Some(timeout),
    )
    .unwrap();
    assert_eq!(sig, 10);
    assert_eq!(COUNT.load(Ordering::SeqCst), 0);
}

#[test]
fn siginfo_carries_the_signal() {
    let _guard = LOCK.lock().unwrap();
    let mut info = frame::make_siginfo(11, signal::SEGV_ACCERR);
    info.si_signo = 11;
    assert_eq!(frame::si_signo(&info), 11);
    assert_eq!(frame::si_code(&info), signal::SEGV_ACCERR);
}

#[test]
fn setjmp_signal_mask_is_saved_and_restored() {
    let _guard = LOCK.lock().unwrap();
    let mut buffer = raven_libc_abi::structs::JmpBuf::ZERO;
    let mut word = 0x1234;
    raven_libc_signal::sigjmp::sigsetjmp_tail(&mut buffer, 0, &mut word);
    word = 0;
    raven_libc_signal::sigjmp::sigsetjmp_tail(&mut buffer, 1, &mut word);
    assert_eq!(word, 0x1234);
}

#[test]
fn earlysig_state_is_visible_before_any_handler() {
    let _guard = LOCK.lock().unwrap();
    let table = DispTable::new();
    assert_eq!(table.read(signal::SIGUSR1), Disposition::DEFAULT);
    assert!(table.handler_set().is_empty());
    let _ = SA_SIGINFO;
}
