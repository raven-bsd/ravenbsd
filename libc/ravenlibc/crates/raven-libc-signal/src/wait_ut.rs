// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

use super::*;
use crate::dispositions::{DispTable, Disposition};
use crate::host_ut::HostSigKernel;
use crate::mask;
use raven_libc_abi::generated::signal;
use raven_libc_abi::structs::SiginfoT;
use std::sync::atomic::{AtomicUsize, Ordering};

static COUNT: AtomicUsize = AtomicUsize::new(0);

extern "C" fn count_handler(_sig: i32) {
    COUNT.fetch_add(1, Ordering::SeqCst);
}

#[test]
fn sigtimedwait_consumes_without_running_the_handler() {
    let _guard = crate::host_ut::GLOBAL_TEST_LOCK.lock().unwrap();
    COUNT.store(0, Ordering::SeqCst);
    let kernel = HostSigKernel::new();
    let table = DispTable::new();
    table.write(
        10,
        Disposition::handler(count_handler as *const () as usize),
    );
    mask::set_mask(&kernel, 0);
    crate::raw::pending_or(kernel.pointer(), crate::critical::signal_bit(10));
    let mut info = SiginfoT::ZERO;
    info.si_signo = 10;
    crate::raw::store_pending_info(kernel.pointer(), &info);
    let mut set = SigSet::empty();
    set.add(10).unwrap();
    let timeout = Timespec {
        tv_sec: 0,
        tv_nsec: 0,
    };
    let mut out = SiginfoT::ZERO;
    let result = sigtimedwait(&kernel, &set, Some(&mut out), Some(timeout)).unwrap();
    assert_eq!(result, 10);
    assert_eq!(out.si_signo, 10);
    assert_eq!(COUNT.load(Ordering::SeqCst), 0);
    assert_eq!(
        crate::raw::pending_load(kernel.pointer()) & crate::critical::signal_bit(10),
        0
    );
}

#[test]
fn zero_timeout_without_pending_is_eagain() {
    let _guard = crate::host_ut::GLOBAL_TEST_LOCK.lock().unwrap();
    let kernel = HostSigKernel::new();
    let mut set = SigSet::empty();
    set.add(1).unwrap();
    let timeout = Timespec {
        tv_sec: 0,
        tv_nsec: 0,
    };
    assert_eq!(
        sigtimedwait(&kernel, &set, None, Some(timeout)),
        Err(EAGAIN_ERR)
    );
}

#[test]
fn negative_timeout_is_einval() {
    let _guard = crate::host_ut::GLOBAL_TEST_LOCK.lock().unwrap();
    let kernel = HostSigKernel::new();
    let set = SigSet::singleton(1);
    let bad = Timespec {
        tv_sec: -1,
        tv_nsec: 0,
    };
    assert_eq!(
        sigtimedwait(&kernel, &set, None, Some(bad)),
        Err(EINVAL_ERR)
    );
    let bad = Timespec {
        tv_sec: 0,
        tv_nsec: -1,
    };
    assert_eq!(
        sigtimedwait(&kernel, &set, None, Some(bad)),
        Err(EINVAL_ERR)
    );
}

#[test]
fn timeout_maps_to_eagain() {
    let _guard = crate::host_ut::GLOBAL_TEST_LOCK.lock().unwrap();
    let kernel = HostSigKernel::new();
    let set = SigSet::singleton(1);
    let timeout = Timespec {
        tv_sec: 1,
        tv_nsec: 0,
    };
    assert_eq!(
        sigtimedwait(&kernel, &set, None, Some(timeout)),
        Err(EAGAIN_ERR)
    );
}

#[test]
fn process_pending_is_consumed() {
    let _guard = crate::host_ut::GLOBAL_TEST_LOCK.lock().unwrap();
    let kernel = HostSigKernel::new();
    let mut record = SiginfoT::ZERO;
    record.si_signo = 17;
    kernel.push_pending(17, record);
    let set = SigSet::singleton(17);
    let timeout = Timespec {
        tv_sec: 0,
        tv_nsec: 0,
    };
    let mut out = SiginfoT::ZERO;
    assert_eq!(
        sigtimedwait(&kernel, &set, Some(&mut out), Some(timeout)),
        Ok(17)
    );
    assert_eq!(out.si_signo, 17);
}

#[test]
fn sigwait_and_sigwaitinfo_use_the_same_path() {
    let _guard = crate::host_ut::GLOBAL_TEST_LOCK.lock().unwrap();
    let kernel = HostSigKernel::new();
    crate::raw::pending_or(kernel.pointer(), crate::critical::signal_bit(5));
    let mut info = SiginfoT::ZERO;
    info.si_signo = 5;
    crate::raw::store_pending_info(kernel.pointer(), &info);
    assert_eq!(sigwait(&kernel, &SigSet::singleton(5)), Ok(5));
    crate::raw::pending_or(kernel.pointer(), crate::critical::signal_bit(6));
    let mut record = SiginfoT::ZERO;
    record.si_signo = 6;
    crate::raw::store_pending_info(kernel.pointer(), &record);
    let mut got = SiginfoT::ZERO;
    assert_eq!(sigwaitinfo(&kernel, &SigSet::singleton(6), &mut got), Ok(6));
    assert_eq!(got.si_signo, 6);
}

#[test]
fn sigsuspend_runs_the_handler_and_returns_eintr() {
    let _guard = crate::host_ut::GLOBAL_TEST_LOCK.lock().unwrap();
    COUNT.store(0, Ordering::SeqCst);
    let kernel = HostSigKernel::new();
    let table = DispTable::new();
    table.write(
        14,
        Disposition::handler(count_handler as *const () as usize),
    );
    mask::set_mask(&kernel, crate::critical::signal_bit(14));
    crate::raw::pending_or(kernel.pointer(), crate::critical::signal_bit(14));
    let set = SigSet::empty();
    let result = sigsuspend(&kernel, &table, &set);
    assert_eq!(result, EINTR_ERR);
    assert_eq!(COUNT.load(Ordering::SeqCst), 1);
    assert_eq!(mask::current_mask(&kernel), crate::critical::signal_bit(14));
}

#[test]
fn sigsuspend_with_a_sticky_kick_does_not_lose_it() {
    let _guard = crate::host_ut::GLOBAL_TEST_LOCK.lock().unwrap();
    let kernel = HostSigKernel::new();
    let table = DispTable::new();
    kernel.kick_self(crate::kernel::KICK_CHECK);
    let result = sigsuspend(&kernel, &table, &SigSet::empty());
    assert_eq!(result, EINTR_ERR);
    assert_eq!(kernel.kick_word(), 0);
}

#[test]
fn interrupted_returns_eintr() {
    let _guard = crate::host_ut::GLOBAL_TEST_LOCK.lock().unwrap();
    assert_eq!(interrupted(), EINTR_ERR);
    let _ = signal::SIGUSR1;
}
