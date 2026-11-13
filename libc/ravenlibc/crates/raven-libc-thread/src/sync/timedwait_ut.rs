// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

use super::*;
use crate::kernel::KernelCaps;
use crate::limits::{CLOCK_MONOTONIC, CLOCK_REALTIME};
use core::sync::atomic::{AtomicI32, Ordering};

fn kernel_with(caps: KernelCaps) -> crate::test_support::MockKernel {
    crate::test_support::MockKernel::with_caps(caps)
}

#[test]
fn no_deadline_waits_forever() {
    let kernel = kernel_with(KernelCaps::full());
    let word = AtomicI32::new(1);
    assert_eq!(
        timedwait_cp(&kernel, &word, 1, CLOCK_REALTIME, None),
        Ok(())
    );
    assert_eq!(kernel.wait_calls(), 1);
    assert_eq!(kernel.last_timeout(), u64::MAX);
}

#[test]
fn a_past_deadline_returns_before_the_kernel_call() {
    let kernel = kernel_with(KernelCaps::full());
    kernel.set_now(1_000);
    let word = AtomicI32::new(1);
    let at = Timespec {
        tv_sec: 0,
        tv_nsec: 999,
    };
    assert_eq!(
        timedwait_cp(&kernel, &word, 1, CLOCK_REALTIME, Some(at)),
        Err(ETIMEDOUT)
    );
    assert_eq!(kernel.wait_calls(), 0);
}

#[test]
fn an_equal_deadline_is_timed_out() {
    let kernel = kernel_with(KernelCaps::full());
    kernel.set_now(1_000);
    let word = AtomicI32::new(1);
    let at = Timespec {
        tv_sec: 0,
        tv_nsec: 1_000,
    };
    assert_eq!(
        timedwait_cp(&kernel, &word, 1, CLOCK_REALTIME, Some(at)),
        Err(ETIMEDOUT)
    );
    assert_eq!(kernel.wait_calls(), 0);
}

#[test]
fn a_future_deadline_becomes_a_relative_timeout() {
    let kernel = kernel_with(KernelCaps::full());
    kernel.set_now(1_000);
    let word = AtomicI32::new(1);
    let at = Timespec {
        tv_sec: 0,
        tv_nsec: 1_500,
    };
    assert_eq!(
        timedwait_cp(&kernel, &word, 1, CLOCK_REALTIME, Some(at)),
        Ok(())
    );
    assert_eq!(kernel.last_timeout(), 500);
}

#[test]
fn a_deadline_after_a_second_borrow() {
    let kernel = kernel_with(KernelCaps::full());
    kernel.set_now(1_000_000);
    let word = AtomicI32::new(1);
    let at = Timespec {
        tv_sec: 2,
        tv_nsec: 250_000,
    };
    assert_eq!(
        timedwait_cp(&kernel, &word, 1, CLOCK_REALTIME, Some(at)),
        Ok(())
    );
    assert_eq!(kernel.last_timeout(), 1_999_250_000);
}

#[test]
fn an_invalid_nanosecond_field_is_einval() {
    let kernel = kernel_with(KernelCaps::full());
    let word = AtomicI32::new(1);
    let at = Timespec {
        tv_sec: 0,
        tv_nsec: 2_000_000_000,
    };
    assert_eq!(
        timedwait_cp(&kernel, &word, 1, CLOCK_REALTIME, Some(at)),
        Err(EINVAL)
    );
    assert_eq!(kernel.wait_calls(), 0);
}

#[test]
fn eintr_passes_with_caps_eintr() {
    let kernel = kernel_with(KernelCaps::full());
    kernel.set_wait_result(4);
    let word = AtomicI32::new(1);
    assert_eq!(
        timedwait_cp(&kernel, &word, 1, CLOCK_MONOTONIC, None),
        Err(EINTR)
    );
}

#[test]
fn eintr_is_spurious_without_caps_eintr() {
    let kernel = kernel_with(KernelCaps::minimal());
    kernel.set_wait_result(4);
    let word = AtomicI32::new(1);
    assert_eq!(
        timedwait_cp(&kernel, &word, 1, CLOCK_MONOTONIC, None),
        Ok(())
    );
}

#[test]
fn etimedout_passes_through() {
    let kernel = kernel_with(KernelCaps::full());
    kernel.set_wait_result(110);
    let word = AtomicI32::new(1);
    assert_eq!(
        timedwait_cp(&kernel, &word, 1, CLOCK_MONOTONIC, None),
        Err(ETIMEDOUT)
    );
}

#[test]
fn ecanceled_passes_through() {
    let kernel = kernel_with(KernelCaps::full());
    kernel.set_wait_result(125);
    let word = AtomicI32::new(1);
    assert_eq!(
        timedwait_cp(&kernel, &word, 1, CLOCK_MONOTONIC, None),
        Err(ECANCELED)
    );
}

#[test]
fn other_kernel_errors_are_spurious() {
    for error in [22, 38, 95, 1] {
        let kernel = kernel_with(KernelCaps::full());
        kernel.set_wait_result(error);
        let word = AtomicI32::new(1);
        assert_eq!(
            timedwait_cp(&kernel, &word, 1, CLOCK_MONOTONIC, None),
            Ok(()),
            "error {error}"
        );
    }
}

#[test]
fn timedwait_disables_and_restores_cancellation() {
    let kernel = kernel_with(KernelCaps::full());
    let pthread = crate::thread::current_pthread(&kernel);
    pthread
        .cancel
        .store(crate::limits::CANCEL_ENABLE, Ordering::Release);
    let word = AtomicI32::new(1);
    assert_eq!(
        timedwait(&kernel, &word, 1, CLOCK_REALTIME, None, true),
        Ok(())
    );
    assert_eq!(
        pthread.cancel.load(Ordering::Acquire) & crate::limits::CANCEL_ENABLE,
        crate::limits::CANCEL_ENABLE
    );
}

#[test]
fn a_zero_timeout_is_never_given_to_the_kernel() {
    let kernel = kernel_with(KernelCaps::full());
    kernel.set_now(500);
    let word = AtomicI32::new(1);
    let at = Timespec {
        tv_sec: 0,
        tv_nsec: 500,
    };
    assert_eq!(
        timedwait_cp(&kernel, &word, 1, CLOCK_REALTIME, Some(at)),
        Err(ETIMEDOUT)
    );
    let at = Timespec {
        tv_sec: 0,
        tv_nsec: 501,
    };
    assert_eq!(
        timedwait_cp(&kernel, &word, 1, CLOCK_REALTIME, Some(at)),
        Ok(())
    );
    assert_eq!(kernel.last_timeout(), 1);
}
