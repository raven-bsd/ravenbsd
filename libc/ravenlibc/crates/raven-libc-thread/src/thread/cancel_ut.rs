// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

use super::*;
use crate::kernel::KernelCaps;
use core::sync::atomic::{AtomicUsize, Ordering};

static CLEANUPS: AtomicUsize = AtomicUsize::new(0);

fn cleanup_fn(_arg: usize) {
    CLEANUPS.fetch_add(1, Ordering::AcqRel);
}

struct Frame {
    pc: usize,
}

impl KickContext for Frame {
    fn pc(&self) -> usize {
        self.pc
    }
    fn set_pc(&mut self, pc: usize) {
        self.pc = pc;
    }
}

fn kernel() -> crate::test_support::MockKernel {
    crate::test_support::MockKernel::new()
}

#[test]
fn setcancelstate_round_trip() {
    let kernel = kernel();
    assert_eq!(
        setcancelstate(&kernel, PTHREAD_CANCEL_DISABLE),
        Ok(PTHREAD_CANCEL_ENABLE)
    );
    assert_eq!(
        setcancelstate(&kernel, PTHREAD_CANCEL_ENABLE),
        Ok(PTHREAD_CANCEL_DISABLE)
    );
    assert_eq!(
        setcancelstate(&kernel, PTHREAD_CANCEL_MASKED),
        Ok(PTHREAD_CANCEL_ENABLE)
    );
    assert_eq!(
        setcancelstate(&kernel, PTHREAD_CANCEL_ENABLE),
        Ok(PTHREAD_CANCEL_MASKED)
    );
}

#[test]
fn setcancelstate_rejects_a_bad_value() {
    let kernel = kernel();
    assert_eq!(setcancelstate(&kernel, 3), Err(EINVAL));
    assert_eq!(setcancelstate(&kernel, -1), Err(EINVAL));
}

#[test]
fn setcanceltype_round_trip() {
    let kernel = kernel();
    assert_eq!(
        setcanceltype(&kernel, PTHREAD_CANCEL_ASYNCHRONOUS),
        Ok(PTHREAD_CANCEL_DEFERRED)
    );
    assert_eq!(
        setcanceltype(&kernel, PTHREAD_CANCEL_DEFERRED),
        Ok(PTHREAD_CANCEL_ASYNCHRONOUS)
    );
    assert_eq!(setcanceltype(&kernel, 2), Err(EINVAL));
}

#[test]
fn no_request_means_no_pending_cancellation() {
    let kernel = kernel();
    let pthread = crate::thread::current_pthread(&kernel);
    pthread
        .cancel
        .store(crate::limits::CANCEL_ENABLE, Ordering::Release);
    assert!(!cancellation_pending(&kernel));
    testcancel(&kernel);
}

#[test]
fn a_disabled_thread_ignores_a_request() {
    let kernel = kernel();
    let pthread = crate::thread::current_pthread(&kernel);
    pthread
        .cancel
        .store(crate::limits::CANCEL_REQUESTED, Ordering::Release);
    testcancel(&kernel);
}

#[test]
fn a_masked_thread_defers() {
    let kernel = kernel();
    let pthread = crate::thread::current_pthread(&kernel);
    pthread.cancel.store(
        crate::limits::CANCEL_ENABLE
            | crate::limits::CANCEL_REQUESTED
            | crate::limits::CANCEL_MASKED,
        Ordering::Release,
    );
    testcancel(&kernel);
}

#[test]
fn an_enabled_request_exits() {
    let _serial = crate::test_support::serial();
    let kernel = kernel();
    let pthread = crate::thread::current_pthread(&kernel);
    pthread.cancel.store(
        crate::limits::CANCEL_ENABLE | crate::limits::CANCEL_REQUESTED,
        Ordering::Release,
    );
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| testcancel(&kernel)));
    assert!(result.is_err());
    pthread
        .cancel
        .store(crate::limits::CANCEL_ENABLE, Ordering::Release);
}

#[test]
fn cancel_records_the_request_without_pg7() {
    let _serial = crate::test_support::serial();
    let kernel = crate::test_support::MockKernel::with_caps(KernelCaps::minimal());
    let pthread = crate::thread::current_pthread_mut(&kernel);
    pthread.tid.store(100, Ordering::Release);
    pthread.handle = 5;
    assert_eq!(cancel(&kernel, kernel.tcb()), Ok(()));
    assert_eq!(
        pthread.cancel.load(Ordering::Acquire) & crate::limits::CANCEL_REQUESTED,
        crate::limits::CANCEL_REQUESTED
    );
}

#[test]
fn cancel_of_a_null_thread_is_esrch() {
    let kernel = kernel();
    assert_eq!(cancel(&kernel, 0), Err(ESRCH));
}

#[test]
fn on_kick_does_nothing_without_a_request() {
    let kernel = kernel();
    let mut frame = Frame { pc: 0 };
    assert_eq!(on_kick(&kernel, &mut frame), KickAction::Nothing);
}

#[test]
fn on_kick_cancels_asynchronously() {
    let kernel = kernel();
    let pthread = crate::thread::current_pthread(&kernel);
    pthread.cancel.store(
        crate::limits::CANCEL_ENABLE
            | crate::limits::CANCEL_REQUESTED
            | crate::limits::CANCEL_ASYNC,
        Ordering::Release,
    );
    let mut frame = Frame { pc: 0 };
    assert_eq!(on_kick(&kernel, &mut frame), KickAction::CancelNow);
    pthread
        .cancel
        .store(crate::limits::CANCEL_ENABLE, Ordering::Release);
}

#[test]
fn on_kick_rewrites_only_inside_the_window() {
    let kernel = kernel();
    let pthread = crate::thread::current_pthread(&kernel);
    pthread.cancel.store(
        crate::limits::CANCEL_ENABLE | crate::limits::CANCEL_REQUESTED,
        Ordering::Release,
    );
    set_cancel_window(0x1000, 0x1100, 0x2000);
    let mut inside = Frame { pc: 0x1040 };
    assert_eq!(on_kick(&kernel, &mut inside), KickAction::RewritePc(0x2000));
    assert_eq!(inside.pc, 0x2000);
    let mut outside = Frame { pc: 0x1200 };
    assert_eq!(on_kick(&kernel, &mut outside), KickAction::Nothing);
    assert_eq!(outside.pc, 0x1200);
    set_cancel_window(0, 0, 0);
    pthread
        .cancel
        .store(crate::limits::CANCEL_ENABLE, Ordering::Release);
}

#[test]
fn cleanup_handlers_run_in_lifo_order() {
    let _serial = crate::test_support::serial();
    CLEANUPS.store(0, Ordering::Release);
    let kernel = kernel();
    let mut first = Cleanup {
        next: 0,
        function: cleanup_fn,
        arg: 1,
    };
    let mut second = Cleanup {
        next: 0,
        function: cleanup_fn,
        arg: 2,
    };
    push_cleanup(&kernel, &mut first);
    push_cleanup(&kernel, &mut second);
    run_cleanup(&kernel);
    assert_eq!(CLEANUPS.load(Ordering::Acquire), 2);
    assert_eq!(crate::thread::current_pthread(&kernel).cancelbuf, 0);
}

#[test]
fn popping_without_running_removes_the_node() {
    let _serial = crate::test_support::serial();
    CLEANUPS.store(0, Ordering::Release);
    let kernel = kernel();
    let mut node = Cleanup {
        next: 0,
        function: cleanup_fn,
        arg: 0,
    };
    push_cleanup(&kernel, &mut node);
    pop_cleanup(&kernel, &mut node, false);
    run_cleanup(&kernel);
    assert_eq!(CLEANUPS.load(Ordering::Acquire), 0);
}

#[test]
fn popping_with_running_calls_the_function() {
    let _serial = crate::test_support::serial();
    CLEANUPS.store(0, Ordering::Release);
    let kernel = kernel();
    let mut node = Cleanup {
        next: 0,
        function: cleanup_fn,
        arg: 0,
    };
    push_cleanup(&kernel, &mut node);
    pop_cleanup(&kernel, &mut node, true);
    assert_eq!(CLEANUPS.load(Ordering::Acquire), 1);
}

#[test]
fn the_cancel_window_is_readable() {
    set_cancel_window(1, 2, 3);
    assert_eq!(cancel_window(), (1, 2, 3));
    set_cancel_window(0, 0, 0);
}

#[test]
fn the_sigcancel_bit_is_signal_33() {
    assert_eq!(SIGCANCEL_BIT, 1 << 32);
}
