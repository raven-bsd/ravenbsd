// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

use super::*;
use crate::dispositions::{DispTable, Disposition};
use crate::host_ut::{HostSigKernel, Trace};
use crate::mask;
use crate::sigset::SigSet;
use raven_libc_abi::structs::Sigval;
use std::sync::atomic::{AtomicUsize, Ordering};

static RAISE_COUNT: AtomicUsize = AtomicUsize::new(0);

extern "C" fn count_handler(_sig: i32) {
    RAISE_COUNT.fetch_add(1, Ordering::SeqCst);
}

#[test]
fn raise_runs_the_handler_before_returning() {
    let _guard = crate::host_ut::GLOBAL_TEST_LOCK.lock().unwrap();
    RAISE_COUNT.store(0, Ordering::SeqCst);
    let kernel = HostSigKernel::new();
    let table = DispTable::new();
    mask::set_mask(&kernel, 0);
    table.write(
        10,
        Disposition::handler(count_handler as *const () as usize),
    );
    raise(&kernel, &table, 10).unwrap();
    assert_eq!(RAISE_COUNT.load(Ordering::SeqCst), 1);
    assert_eq!(
        crate::raw::pending_load(kernel.pointer()) & crate::critical::signal_bit(10),
        0
    );
}

#[test]
fn raise_while_masked_leaves_it_pending() {
    let _guard = crate::host_ut::GLOBAL_TEST_LOCK.lock().unwrap();
    RAISE_COUNT.store(0, Ordering::SeqCst);
    let kernel = HostSigKernel::new();
    let table = DispTable::new();
    mask::set_mask(&kernel, crate::critical::signal_bit(12));
    table.write(
        12,
        Disposition::handler(count_handler as *const () as usize),
    );
    raise(&kernel, &table, 12).unwrap();
    assert_eq!(RAISE_COUNT.load(Ordering::SeqCst), 0);
    assert_ne!(
        crate::raw::pending_load(kernel.pointer()) & crate::critical::signal_bit(12),
        0
    );
    crate::maskops::sigprocmask(
        &kernel,
        raven_libc_abi::generated::signal::SIG_UNBLOCK,
        Some(&SigSet::singleton(12)),
        None,
    )
    .unwrap();
    let point = kernel.safe_point(&table, None, None);
    assert!(point.terminated.is_none());
    assert_eq!(RAISE_COUNT.load(Ordering::SeqCst), 1);
}

#[test]
fn raise_validates_the_signal() {
    let _guard = crate::host_ut::GLOBAL_TEST_LOCK.lock().unwrap();
    let kernel = HostSigKernel::new();
    let table = DispTable::new();
    for sig in [0, -1, 32, 33, 34, 65] {
        assert_eq!(raise(&kernel, &table, sig), Err(EINVAL_ERR));
    }
}

#[test]
fn kill_forwards_to_procd() {
    let _guard = crate::host_ut::GLOBAL_TEST_LOCK.lock().unwrap();
    let kernel = HostSigKernel::new();
    kill(&kernel, 99, 15).unwrap();
    assert!(kernel.trace().contains(&Trace::ProcKill(99, 15)));
    kill(&kernel, 99, 0).unwrap();
    assert!(kernel.trace().contains(&Trace::ProcKill(99, 0)));
}

#[test]
fn kill_without_proc_kill_is_enosys_but_raise_works() {
    let _guard = crate::host_ut::GLOBAL_TEST_LOCK.lock().unwrap();
    RAISE_COUNT.store(0, Ordering::SeqCst);
    let kernel = HostSigKernel::new();
    kernel.disable_proc_kill();
    let table = DispTable::new();
    mask::set_mask(&kernel, 0);
    table.write(
        10,
        Disposition::handler(count_handler as *const () as usize),
    );
    assert_eq!(kill(&kernel, 99, 15), Err(ENOSYS_ERR));
    raise(&kernel, &table, 10).unwrap();
    assert_eq!(RAISE_COUNT.load(Ordering::SeqCst), 1);
}

#[test]
fn killpg_rejects_bad_groups() {
    let _guard = crate::host_ut::GLOBAL_TEST_LOCK.lock().unwrap();
    let kernel = HostSigKernel::new();
    for group in [0, -1, i32::MIN] {
        assert_eq!(killpg(&kernel, group, 15), Err(EINVAL_ERR));
    }
    killpg(&kernel, 5, 15).unwrap();
    assert!(kernel.trace().contains(&Trace::ProcKill(-5, 15)));
}

#[test]
fn sigqueue_builds_a_si_queue_record() {
    let _guard = crate::host_ut::GLOBAL_TEST_LOCK.lock().unwrap();
    let kernel = HostSigKernel::new();
    sigqueue(&kernel, 99, 40, Sigval { bytes: [0xfeed] }).unwrap();
    let (pid, sig, info) = kernel.last_proc_info().unwrap();
    assert_eq!((pid, sig), (99, 40));
    assert_eq!(
        crate::frame::si_code(&info),
        raven_libc_abi::generated::signal::SI_QUEUE
    );
    assert_eq!(crate::frame::si_value(&info).bytes[0], 0xfeed);
}

#[test]
fn sigqueue_rejects_internal_and_needs_proc_kill() {
    let _guard = crate::host_ut::GLOBAL_TEST_LOCK.lock().unwrap();
    let kernel = HostSigKernel::new();
    assert_eq!(sigqueue(&kernel, 1, 33, Sigval::ZERO), Err(EINVAL_ERR));
    kernel.disable_proc_kill();
    assert_eq!(sigqueue(&kernel, 1, 15, Sigval::ZERO), Err(ENOSYS_ERR));
}

#[test]
fn application_mask_is_the_app_word() {
    let _guard = crate::host_ut::GLOBAL_TEST_LOCK.lock().unwrap();
    assert_eq!(application_mask(u64::MAX), crate::sigset::APP_MASK);
    assert!(blocked(crate::critical::signal_bit(2), 2));
}
