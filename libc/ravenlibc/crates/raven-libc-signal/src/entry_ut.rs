// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

use crate::dispositions::{DispTable, Disposition};
use crate::host_ut::HostSigKernel;
use crate::kernel::{FaultKind, FaultRecord};
use crate::mask;
use raven_libc_abi::structs::SiginfoT;
use std::sync::atomic::{AtomicUsize, Ordering};

static CALLS: AtomicUsize = AtomicUsize::new(0);
static LAST_SIG: AtomicUsize = AtomicUsize::new(0);
static SP: AtomicUsize = AtomicUsize::new(0);

extern "C" fn plain(sig: i32) {
    CALLS.fetch_add(1, Ordering::SeqCst);
    LAST_SIG.store(sig as usize, Ordering::SeqCst);
    SP.store(crate::context::current_stack_pointer(), Ordering::SeqCst);
}

extern "C" fn with_info(
    sig: i32,
    info: *const SiginfoT,
    context: *mut raven_libc_abi::structs::UcontextT,
) {
    CALLS.fetch_add(1, Ordering::SeqCst);
    LAST_SIG.store(sig as usize, Ordering::SeqCst);
    // SAFETY: the entry passes live stack objects for the duration of the call.
    unsafe {
        if !info.is_null() && !context.is_null() {
            SP.store((*info).si_signo as usize, Ordering::SeqCst);
        }
    }
}

#[test]
fn a_plain_handler_runs_once() {
    let _guard = crate::host_ut::GLOBAL_TEST_LOCK.lock().unwrap();
    CALLS.store(0, Ordering::SeqCst);
    let kernel = HostSigKernel::new();
    let table = DispTable::new();
    mask::set_mask(&kernel, 0);
    table.write(10, Disposition::handler(plain as *const () as usize));
    let outcome = kernel
        .direct(&table, crate::kernel::KICK_CHECK, None)
        .unwrap();
    let _ = outcome;
    crate::raise(&kernel, &table, 10).unwrap();
    assert_eq!(CALLS.load(Ordering::SeqCst), 1);
    assert_eq!(LAST_SIG.load(Ordering::SeqCst), 10);
}

#[test]
fn sig_ign_and_default_ignore_do_nothing() {
    let _guard = crate::host_ut::GLOBAL_TEST_LOCK.lock().unwrap();
    let kernel = HostSigKernel::new();
    let table = DispTable::new();
    mask::set_mask(&kernel, 0);
    table.write(1, Disposition::IGNORE);
    crate::raise(&kernel, &table, 1).unwrap();
    crate::raise(&kernel, &table, 17).unwrap(); // SIGCHLD default is ignore
}

#[test]
fn a_default_terminate_is_recorded() {
    let _guard = crate::host_ut::GLOBAL_TEST_LOCK.lock().unwrap();
    let kernel = HostSigKernel::new();
    let table = DispTable::new();
    mask::set_mask(&kernel, 0);
    let result = kernel.direct(&table, 0, None);
    assert!(result.is_ok());
    crate::raw::pending_or(kernel.pointer(), crate::critical::signal_bit(15));
    let result = kernel.direct(&table, crate::kernel::KICK_CHECK, None);
    assert_eq!(result, Err((15, false)));
}

#[test]
fn a_blocked_fault_signal_is_still_delivered() {
    let _guard = crate::host_ut::GLOBAL_TEST_LOCK.lock().unwrap();
    CALLS.store(0, Ordering::SeqCst);
    let kernel = HostSigKernel::new();
    let table = DispTable::new();
    mask::set_mask(&kernel, crate::critical::signal_bit(11));
    table.write(11, Disposition::handler(plain as *const () as usize));
    let fault = FaultRecord {
        kind: FaultKind::PageNomap as u32,
        addr: 0xdead_0000,
        pc: 0x1000,
    };
    kernel
        .direct(&table, crate::kernel::KICK_CHECK, Some(fault))
        .unwrap();
    assert_eq!(CALLS.load(Ordering::SeqCst), 1);
    assert_eq!(LAST_SIG.load(Ordering::SeqCst), 11);
}

#[test]
fn a_default_fault_terminates_with_core() {
    let _guard = crate::host_ut::GLOBAL_TEST_LOCK.lock().unwrap();
    let kernel = HostSigKernel::new();
    let table = DispTable::new();
    mask::set_mask(&kernel, 0);
    let fault = FaultRecord {
        kind: FaultKind::PagePerm as u32,
        addr: 1,
        pc: 2,
    };
    assert_eq!(
        kernel.direct(&table, crate::kernel::KICK_CHECK, Some(fault)),
        Err((11, true))
    );
}

#[test]
fn a_spurious_kick_delivers_nothing() {
    let _guard = crate::host_ut::GLOBAL_TEST_LOCK.lock().unwrap();
    let kernel = HostSigKernel::new();
    let table = DispTable::new();
    mask::set_mask(&kernel, 0);
    let outcome = kernel
        .direct(&table, crate::kernel::KICK_CHECK, None)
        .unwrap();
    assert!(outcome.spurious);
    assert_eq!(outcome.delivered, 0);
}

#[test]
fn errno_is_preserved_across_a_handler() {
    let _guard = crate::host_ut::GLOBAL_TEST_LOCK.lock().unwrap();
    extern "C" fn clobber_errno(_sig: i32) {
        raven_libc_sys::errno::set_errno(9);
    }
    let kernel = HostSigKernel::new();
    let table = DispTable::new();
    mask::set_mask(&kernel, 0);
    table.write(
        12,
        Disposition::handler(clobber_errno as *const () as usize),
    );
    raven_libc_sys::errno::set_errno(123);
    crate::raise(&kernel, &table, 12).unwrap();
    assert_eq!(raven_libc_sys::errno::errno(), 123);
}

#[test]
fn sa_siginfo_receives_the_record_and_context() {
    let _guard = crate::host_ut::GLOBAL_TEST_LOCK.lock().unwrap();
    CALLS.store(0, Ordering::SeqCst);
    let kernel = HostSigKernel::new();
    let table = DispTable::new();
    mask::set_mask(&kernel, 0);
    let mut action = Disposition::handler(with_info as *const () as usize);
    action.flags = crate::dispositions::SA_SIGINFO;
    table.write(10, action);
    crate::raise(&kernel, &table, 10).unwrap();
    assert_eq!(CALLS.load(Ordering::SeqCst), 1);
    assert_eq!(SP.load(Ordering::SeqCst), 10, "si_signo");
}

#[test]
fn sa_onstack_switches_the_handler_stack() {
    let _guard = crate::host_ut::GLOBAL_TEST_LOCK.lock().unwrap();
    CALLS.store(0, Ordering::SeqCst);
    let kernel = HostSigKernel::new();
    let table = DispTable::new();
    mask::set_mask(&kernel, 0);
    let buffer = vec![0u8; 32 * 1024];
    let base = buffer.as_ptr() as usize;
    let stack = raven_libc_abi::structs::StackT {
        ss_sp: base as *mut core::ffi::c_void,
        ss_flags: 0,
        ss_size: 32 * 1024,
    };
    crate::altstack::sigaltstack(&kernel, Some(&stack), None).unwrap();
    let mut action = Disposition::handler(plain as *const () as usize);
    action.flags = crate::dispositions::SA_ONSTACK;
    table.write(10, action);
    crate::raise(&kernel, &table, 10).unwrap();
    let sp = SP.load(Ordering::SeqCst);
    assert!(
        sp >= base && sp < base + 32 * 1024,
        "handler sp {sp:#x} not in altstack"
    );
}

#[test]
fn a_kick_inside_a_critical_section_defers() {
    let _guard = crate::host_ut::GLOBAL_TEST_LOCK.lock().unwrap();
    let kernel = HostSigKernel::new();
    let table = DispTable::new();
    mask::set_mask(&kernel, 0);
    table.write(10, Disposition::handler(plain as *const () as usize));
    crate::critical::critical_enter(&kernel);
    crate::raise(&kernel, &table, 10).unwrap();
    let outcome = kernel
        .direct(&table, crate::kernel::KICK_CHECK, None)
        .unwrap();
    assert_eq!(outcome.delivered, 0);
    assert_ne!(crate::critical::snapshot(kernel.pointer()).2, 0);
}
