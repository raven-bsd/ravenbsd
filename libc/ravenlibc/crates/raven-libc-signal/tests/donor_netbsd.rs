// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Donor-test ports: NetBSD ATF `tests/lib/libc/sys/{t_sigaction,t_sigaltstack,t_sigqueue,
//! t_sigtimedwait}.c`, `tests/lib/libc/gen/{t_raise,t_siginfo}.c` and
//! `tests/lib/libpthread/{t_siglongjmp,t_sigmask,t_sigsuspend,t_kill,t_swapcontext}.c`.
//!
//! The expected values and scenarios are ported to Rust; no NetBSD C is copied. Cases that need
//! a real kernel are documented `not ported: needs PG-n`.

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
use raven_libc_abi::structs::{SiginfoT, Sigval, StackT, UcontextT};
use raven_libc_signal::dispositions::{DispTable, Disposition};
use raven_libc_signal::kernel::{FaultRecord, SigCaps};
use raven_libc_signal::{
    EAGAIN_ERR, EINTR_ERR, EINVAL_ERR, ENOMEM_ERR, ENOSYS_ERR, SigSet, altstack, frame, kill, mask,
    raise, sigaction, sigqueue, sigsuspend, sigtimedwait,
};

static LOCK: Mutex<()> = Mutex::new(());
static COUNT: AtomicUsize = AtomicUsize::new(0);
static CONTEXT_LOOPS: AtomicUsize = AtomicUsize::new(0);

extern "C" fn handler(_sig: i32) {
    COUNT.fetch_add(1, Ordering::SeqCst);
}

fn install(table: &DispTable, sig: i32, flags: u32) {
    table.write(
        sig,
        Disposition {
            handler: handler as *const () as usize,
            mask: 0,
            flags,
        },
    );
}

#[test]
fn t_sigaction_rejects_kill_and_stop() {
    let _guard = LOCK.lock().unwrap();
    let table = DispTable::new();
    assert_eq!(
        sigaction(&table, signal::SIGKILL, Some(&Disposition::IGNORE), None),
        Err(EINVAL_ERR)
    );
    assert_eq!(
        sigaction(&table, signal::SIGSTOP, Some(&Disposition::IGNORE), None),
        Err(EINVAL_ERR)
    );
}

#[test]
fn t_sigaction_old_round_trip_and_restorer_bit() {
    let _guard = LOCK.lock().unwrap();
    let table = DispTable::new();
    let action = Disposition {
        handler: 0x1234,
        mask: 0x10,
        flags: 0x0400_0000 | 0x1000_0000,
    };
    sigaction(&table, signal::SIGUSR1, Some(&action), None).unwrap();
    let mut old = Disposition::DEFAULT;
    sigaction(&table, signal::SIGUSR1, None, Some(&mut old)).unwrap();
    assert_eq!(old.handler, 0x1234);
    assert_eq!(old.flags, 0x1000_0000, "SA_RESTORER must not be stored");
}

#[test]
fn t_sigaltstack_reports_enomem_and_onstack_einval() {
    let _guard = LOCK.lock().unwrap();
    let kernel = HostSigKernel::new();
    let small = StackT {
        ss_sp: 0x1000 as *mut core::ffi::c_void,
        ss_flags: 0,
        ss_size: 1024,
    };
    assert_eq!(
        altstack::sigaltstack(&kernel, Some(&small), None),
        Err(ENOMEM_ERR)
    );
    let mut on = small;
    on.ss_size = 32 * 1024;
    on.ss_flags = altstack::SS_ONSTACK;
    assert_eq!(
        altstack::sigaltstack(&kernel, Some(&on), None),
        Err(EINVAL_ERR)
    );
    let mut old = StackT::ZERO;
    altstack::sigaltstack(&kernel, None, Some(&mut old)).unwrap();
    assert_eq!(old.ss_flags & altstack::SS_DISABLE, altstack::SS_DISABLE);
}

#[test]
fn t_sigqueue_builds_si_queue() {
    let _guard = LOCK.lock().unwrap();
    let kernel = HostSigKernel::new();
    sigqueue(&kernel, common::fake::FAKE_PID, 40, Sigval { bytes: [7] }).unwrap();
    let mut info = SiginfoT::ZERO;
    let timeout = raven_libc_abi::structs::Timespec {
        tv_sec: 0,
        tv_nsec: 0,
    };
    let sig = sigtimedwait(
        &kernel,
        &SigSet::singleton(40),
        Some(&mut info),
        Some(timeout),
    )
    .unwrap();
    assert_eq!(sig, 40);
    assert_eq!(frame::si_code(&info), signal::SI_QUEUE);
    assert_eq!(frame::si_value(&info).bytes[0], 7);
}

#[test]
fn t_sigtimedwait_eagain_on_timeout() {
    let _guard = LOCK.lock().unwrap();
    let kernel = HostSigKernel::new();
    let timeout = raven_libc_abi::structs::Timespec {
        tv_sec: 0,
        tv_nsec: 0,
    };
    assert_eq!(
        sigtimedwait(&kernel, &SigSet::singleton(1), None, Some(timeout)),
        Err(EAGAIN_ERR)
    );
}

#[test]
fn t_raise_runs_handler_before_returning() {
    let _guard = LOCK.lock().unwrap();
    COUNT.store(0, Ordering::SeqCst);
    let kernel = HostSigKernel::new();
    let table = DispTable::new();
    mask::set_mask(&kernel, 0);
    install(&table, 10, 0);
    raise(&kernel, &table, 10).unwrap();
    assert_eq!(COUNT.load(Ordering::SeqCst), 1);
}

#[test]
fn t_sigmask_blocks_and_unblocks() {
    let _guard = LOCK.lock().unwrap();
    let kernel = HostSigKernel::new();
    let set = SigSet::singleton(12);
    let mut old = SigSet::empty();
    raven_libc_signal::sigprocmask(&kernel, signal::SIG_BLOCK, Some(&set), Some(&mut old)).unwrap();
    assert!(mask::is_blocked(mask::current_mask(&kernel), 12));
    assert_eq!(old.app_word(), 0);
    raven_libc_signal::sigprocmask(&kernel, signal::SIG_UNBLOCK, Some(&set), None).unwrap();
    assert!(!mask::is_blocked(mask::current_mask(&kernel), 12));
}

#[test]
fn t_sigsuspend_returns_eintr_after_the_handler() {
    let _guard = LOCK.lock().unwrap();
    COUNT.store(0, Ordering::SeqCst);
    let kernel = HostSigKernel::new();
    let table = DispTable::new();
    install(&table, 14, 0);
    raven_libc_signal::raw::pending_or(
        kernel.pointer(),
        raven_libc_signal::critical::signal_bit(14),
    );
    assert_eq!(sigsuspend(&kernel, &table, &SigSet::empty()), EINTR_ERR);
    assert_eq!(COUNT.load(Ordering::SeqCst), 1);
}

#[test]
fn t_kill_pid_zero_is_an_existence_check() {
    let _guard = LOCK.lock().unwrap();
    let kernel = HostSigKernel::new();
    kill(&kernel, common::fake::FAKE_PID, 0).unwrap();
    assert!(
        kernel
            .trace()
            .contains(&common::fake::Trace::ProcKill(common::fake::FAKE_PID, 0))
    );
}

#[test]
fn t_kill_without_proc_kill_is_enosys() {
    let _guard = LOCK.lock().unwrap();
    let caps = SigCaps {
        proc_kill: false,
        ..SigCaps::HOST
    };
    let kernel = HostSigKernel::with_caps(caps);
    assert_eq!(kill(&kernel, 99, signal::SIGTERM), Err(ENOSYS_ERR));
}

#[test]
fn t_siginfo_fields() {
    let _guard = LOCK.lock().unwrap();
    let mut info = frame::make_siginfo(signal::SIGSEGV, signal::SEGV_MAPERR);
    frame::set_si_addr(&mut info, 0xdead);
    assert_eq!(frame::si_signo(&info), signal::SIGSEGV);
    assert_eq!(frame::si_code(&info), signal::SEGV_MAPERR);
    assert_eq!(frame::si_addr(&info), 0xdead);
}

#[test]
fn t_siglongjmp_tail_restores_the_mask_word() {
    let _guard = LOCK.lock().unwrap();
    let mut buffer = raven_libc_abi::structs::JmpBuf::ZERO;
    let mut word = 0xaa;
    raven_libc_signal::sigjmp::sigsetjmp_tail(&mut buffer, 0, &mut word);
    word = 0xbb;
    raven_libc_signal::sigjmp::sigsetjmp_tail(&mut buffer, 1, &mut word);
    assert_eq!(word, 0xaa);
}

#[test]
fn t_swapcontext_returns_zero_on_resume() {
    let _guard = LOCK.lock().unwrap();
    let mut context = UcontextT::ZERO;
    // Memory, not a callee-saved register: `setcontext_restore` resumes with the register
    // state saved by `getcontext_save`, so a register-resident counter would never advance.
    CONTEXT_LOOPS.store(0, Ordering::SeqCst);
    loop {
        let result = raven_libc_signal::raw::getcontext_save(&mut context);
        assert_eq!(result, 0);
        let loops = CONTEXT_LOOPS.fetch_add(1, Ordering::SeqCst) + 1;
        if loops == 2 {
            break;
        }
        raven_libc_signal::raw::setcontext_restore(&context);
    }
    assert_eq!(CONTEXT_LOOPS.load(Ordering::SeqCst), 2);
}

#[test]
fn not_ported_notes_are_explicit() {
    // t_sigalarm needs a real timer delivery: `not ported: needs PG-19`.
    // t_cancel/t_sigcancel need a real kick: `not ported: needs PG-7`.
    let _ = FaultRecord {
        kind: 0,
        addr: 0,
        pc: 0,
    };
}
