// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Unit tests for the IPI lock-order model.

use super::*;

#[test]
fn raw_spin_across_wait_is_rejected() {
    let trace = [
        Event::Acquire(LockKind::RawSpin),
        Event::WaitForIpi,
        Event::Release(LockKind::RawSpin),
    ];
    assert_eq!(check(&trace), Err(Violation::RawSpinAcrossIpiWait));
}

#[test]
fn release_before_wait_is_allowed() {
    let trace = [
        Event::Acquire(LockKind::RawSpin),
        Event::Release(LockKind::RawSpin),
        Event::WaitForIpi,
    ];
    assert_eq!(check(&trace), Ok(()));
}

#[test]
fn interrupt_safe_active_guard_may_serialise_the_wait() {
    // The shooter-side ActiveGuard re-enables interrupts and the IPI handler never takes it.
    let trace = [
        Event::Acquire(LockKind::InterruptSafe),
        Event::WaitForIpi,
        Event::Release(LockKind::InterruptSafe),
    ];
    assert_eq!(check(&trace), Ok(()));
}

#[test]
fn nested_raw_spin_waits_are_rejected_until_both_are_released() {
    let trace = [
        Event::Acquire(LockKind::RawSpin),
        Event::Acquire(LockKind::RawSpin),
        Event::Release(LockKind::RawSpin),
        Event::WaitForIpi,
        Event::Release(LockKind::RawSpin),
    ];
    assert_eq!(check(&trace), Err(Violation::RawSpinAcrossIpiWait));
    let released = [
        Event::Acquire(LockKind::RawSpin),
        Event::Acquire(LockKind::RawSpin),
        Event::Release(LockKind::RawSpin),
        Event::Release(LockKind::RawSpin),
        Event::WaitForIpi,
    ];
    assert_eq!(check(&released), Ok(()));
}

#[test]
fn masked_spinner_times_out() {
    let target = Target::masked_spinner();
    assert!(target.is_spinning_on_raw_spin());
    assert!(!target.can_acknowledge());
    assert_eq!(target.wait(false), None);
    assert_eq!(acknowledge(target, false), WaitOutcome::TimedOut);
}

#[test]
fn interruptible_spinner_acknowledges() {
    let target = Target::interruptible_spinner();
    assert!(target.is_spinning_on_raw_spin());
    assert!(target.can_acknowledge());
    assert_eq!(target.wait(false), Some(()));
    assert_eq!(acknowledge(target, false), WaitOutcome::Acknowledged);
}

#[test]
fn enabling_interrupts_before_the_deadline_acknowledges() {
    // S113: syscall entry enables IF/DAIF while the peer keeps spinning on the address-space
    // lock, so the TLB IPI handler can run and acknowledge.
    let mut target = Target::masked_spinner();
    assert_eq!(acknowledge(target, false), WaitOutcome::TimedOut);
    target.enable_interrupts();
    assert!(target.can_acknowledge());
    assert_eq!(acknowledge(target, false), WaitOutcome::Acknowledged);
}

#[test]
fn s113_delivery_mitigation_does_not_satisfy_the_lock_rule() {
    // The target can acknowledge after the S113 interrupt enable...
    let target = Target::interruptible_spinner();
    assert_eq!(acknowledge(target, false), WaitOutcome::Acknowledged);
    // ...but the shooter still holds the address-space raw spinlock across the wait.
    let trace = [
        Event::Acquire(LockKind::RawSpin),
        Event::WaitForIpi,
        Event::Release(LockKind::RawSpin),
    ];
    assert_eq!(check(&trace), Err(Violation::RawSpinAcrossIpiWait));
}

#[test]
fn a_masked_target_stays_masked_across_a_repeated_wait() {
    let target = Target::masked_spinner();
    for _ in 0..100 {
        assert_eq!(target.wait(false), None);
    }
    assert!(!target.can_acknowledge());
}
