// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

use super::*;
use crate::dispositions::{DispTable, Disposition};
use crate::host_ut::HostSigKernel;
use crate::kernel::SigCaps;
use crate::mask;
use std::sync::atomic::{AtomicUsize, Ordering};

static COUNT: AtomicUsize = AtomicUsize::new(0);

extern "C" fn count_handler(_sig: i32) {
    COUNT.fetch_add(1, Ordering::SeqCst);
}

#[test]
fn enter_and_exit_update_the_counters() {
    let _guard = crate::host_ut::GLOBAL_TEST_LOCK.lock().unwrap();
    let kernel = HostSigKernel::new();
    crate::critical::init_thread_state(kernel.pointer());
    critical_enter(&kernel);
    critical_enter(&kernel);
    assert_eq!(snapshot(kernel.pointer()).0, 2);
    critical_exit(&kernel, &DispTable::new());
    critical_exit(&kernel, &DispTable::new());
    assert_eq!(snapshot(kernel.pointer()).0, 0);
}

#[test]
fn a_new_thread_starts_deferred() {
    let _guard = crate::host_ut::GLOBAL_TEST_LOCK.lock().unwrap();
    let kernel = HostSigKernel::new();
    crate::critical::init_thread_state(kernel.pointer());
    assert_eq!(snapshot(kernel.pointer()).1, 1);
    defer_exit(&kernel, &DispTable::new());
    assert_eq!(snapshot(kernel.pointer()).1, 0);
}

#[test]
fn a_kick_inside_a_critical_section_defers_and_replays_on_exit() {
    let _guard = crate::host_ut::GLOBAL_TEST_LOCK.lock().unwrap();
    COUNT.store(0, Ordering::SeqCst);
    let kernel = HostSigKernel::new();
    let table = DispTable::new();
    mask::set_mask(&kernel, 0);
    table.write(
        10,
        Disposition::handler(count_handler as *const () as usize),
    );
    critical_enter(&kernel);
    crate::raise(&kernel, &table, 10).unwrap();
    assert_eq!(
        COUNT.load(Ordering::SeqCst),
        0,
        "handler ran inside the critical section"
    );
    assert_eq!(
        snapshot(kernel.pointer()).2,
        crate::critical::signal_bit(10)
    );
    critical_exit(&kernel, &table);
    assert_eq!(
        snapshot(kernel.pointer()).2,
        0,
        "deferred bit must be replayed"
    );
    assert_eq!(
        COUNT.load(Ordering::SeqCst),
        0,
        "critical_exit must never call the handler"
    );
    let point = kernel.safe_point(&table, None, None);
    assert!(point.terminated.is_none());
    assert_eq!(COUNT.load(Ordering::SeqCst), 1);
}

#[test]
fn the_kick_fallback_works_without_sigresend() {
    let _guard = crate::host_ut::GLOBAL_TEST_LOCK.lock().unwrap();
    COUNT.store(0, Ordering::SeqCst);
    let caps = SigCaps {
        resend: false,
        ..SigCaps::HOST
    };
    let kernel = HostSigKernel::with_caps(caps);
    let table = DispTable::new();
    mask::set_mask(&kernel, 0);
    table.write(
        10,
        Disposition::handler(count_handler as *const () as usize),
    );
    critical_enter(&kernel);
    crate::raise(&kernel, &table, 10).unwrap();
    critical_exit(&kernel, &table);
    assert_eq!(COUNT.load(Ordering::SeqCst), 0);
    assert_ne!(kernel.kick_word(), 0);
    kernel.safe_point(&table, None, None);
    assert_eq!(COUNT.load(Ordering::SeqCst), 1);
}

#[test]
fn ten_thousand_iterations_never_lose_or_duplicate() {
    let _guard = crate::host_ut::GLOBAL_TEST_LOCK.lock().unwrap();
    COUNT.store(0, Ordering::SeqCst);
    let kernel = HostSigKernel::new();
    let table = DispTable::new();
    mask::set_mask(&kernel, 0);
    table.write(
        10,
        Disposition::handler(count_handler as *const () as usize),
    );
    let mut expected = 0usize;
    for iteration in 0..10_000u32 {
        critical_enter(&kernel);
        if iteration % 3 != 0 {
            crate::raise(&kernel, &table, 10).unwrap();
            expected = expected.wrapping_add(1);
        }
        critical_exit(&kernel, &table);
        // Let the fake process the replay kick, if any.
        let point = kernel.safe_point(&table, None, None);
        assert!(point.terminated.is_none());
        assert_eq!(
            COUNT.load(Ordering::SeqCst),
            expected,
            "iteration {iteration}: lost or duplicated signal"
        );
        assert_eq!(snapshot(kernel.pointer()).2, 0);
        assert_eq!(
            crate::raw::pending_load(kernel.pointer()) & crate::critical::signal_bit(10),
            0
        );
    }
    assert_eq!(expected, 6_666);
}

#[test]
fn deferred_signal_bit_helper_and_app_mask() {
    let _guard = crate::host_ut::GLOBAL_TEST_LOCK.lock().unwrap();
    assert_eq!(signal_bit(0), 0);
    assert_eq!(signal_bit(65), 0);
    assert_eq!(signal_bit(1), 1);
    assert_eq!(signal_bit(64), 1u64 << 63);
    assert_eq!(app_mask().app_word(), crate::sigset::APP_MASK);
}
