// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host tests for the bounded error-recovery policy.

use super::*;

#[test]
fn the_default_policy_is_bounded() {
    let policy = ErrorRecovery::default();
    assert_eq!(policy.max_consecutive(), 8);
    assert_eq!(policy.max_resets(), 3);
    assert_eq!(policy.consecutive(), 0);
    assert_eq!(policy.resets(), 0);
}

#[test]
fn a_threshold_of_errors_requests_one_ring_reset() {
    let mut policy = ErrorRecovery::new(3, 2);
    assert_eq!(policy.record_error(), RecoveryAction::None);
    assert_eq!(policy.record_error(), RecoveryAction::None);
    assert_eq!(policy.record_error(), RecoveryAction::ResetRings);
    assert_eq!(policy.consecutive(), 0);
    assert_eq!(policy.resets(), 1);
}

#[test]
fn a_success_clears_the_consecutive_counter() {
    let mut policy = ErrorRecovery::new(3, 2);
    policy.record_error();
    policy.record_error();
    policy.record_success();
    assert_eq!(policy.consecutive(), 0);
    assert_eq!(policy.record_error(), RecoveryAction::None);
    assert_eq!(policy.resets(), 0);
}

#[test]
fn the_reset_budget_is_exhausted_to_a_fault() {
    let mut policy = ErrorRecovery::new(1, 2);
    assert_eq!(policy.record_error(), RecoveryAction::ResetRings);
    assert_eq!(policy.record_error(), RecoveryAction::ResetRings);
    assert_eq!(policy.record_error(), RecoveryAction::Fault);
    assert_eq!(policy.record_error(), RecoveryAction::Fault);
    assert_eq!(policy.resets(), 2);
}

#[test]
fn zero_thresholds_are_clamped_to_one() {
    let policy = ErrorRecovery::new(0, 0);
    assert_eq!(policy.max_consecutive(), 1);
    assert_eq!(policy.max_resets(), 1);
    let mut policy = policy;
    assert_eq!(policy.record_error(), RecoveryAction::ResetRings);
    assert_eq!(policy.record_error(), RecoveryAction::Fault);
}

#[test]
fn the_consecutive_counter_saturates() {
    let mut policy = ErrorRecovery::new(u32::MAX, 1);
    policy.record_error();
    assert_eq!(policy.consecutive(), 1);
    for _ in 0..10 {
        policy.record_error();
    }
    assert!(policy.consecutive() > 0);
    assert_eq!(policy.resets(), 0);
}
