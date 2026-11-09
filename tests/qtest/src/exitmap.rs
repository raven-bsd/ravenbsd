// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Mapping of QEMU process exit statuses to guest outcomes (contract C5).

use crate::case::ExpectExit;

/// What the guest signalled.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ExitKind {
    /// `isa-debug-exit` status 33.
    Success,
    /// `isa-debug-exit` status 35.
    Failure,
    /// Any other status, a signal, or no status at all.
    Crash,
}

/// Map a process exit status to the guest outcome.
pub(crate) fn classify(status: Option<i32>) -> ExitKind {
    match status {
        Some(33) => ExitKind::Success,
        Some(35) => ExitKind::Failure,
        _ => ExitKind::Crash,
    }
}

/// Whether a normal (non-timeout) exit satisfies `expect_exit`.
pub(crate) fn expected_matches(expected: ExpectExit, kind: ExitKind) -> bool {
    match expected {
        ExpectExit::Success => kind == ExitKind::Success,
        ExpectExit::Failure => kind == ExitKind::Failure,
        ExpectExit::Any => true,
        ExpectExit::Timeout => false,
    }
}

/// Whether a timed-out run satisfies `expect_exit`.
pub(crate) fn timeout_matches(expected: ExpectExit) -> bool {
    expected == ExpectExit::Timeout
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exit_33_is_success() {
        assert_eq!(classify(Some(33)), ExitKind::Success);
    }

    #[test]
    fn exit_35_is_failure() {
        assert_eq!(classify(Some(35)), ExitKind::Failure);
    }

    #[test]
    fn every_other_status_is_crash() {
        for status in [Some(0), Some(1), Some(77), Some(137), None] {
            assert_eq!(classify(status), ExitKind::Crash, "{status:?}");
        }
    }

    #[test]
    fn success_expectation() {
        assert!(expected_matches(ExpectExit::Success, ExitKind::Success));
        assert!(!expected_matches(ExpectExit::Success, ExitKind::Failure));
        assert!(!expected_matches(ExpectExit::Success, ExitKind::Crash));
    }

    #[test]
    fn failure_expectation() {
        assert!(expected_matches(ExpectExit::Failure, ExitKind::Failure));
        assert!(!expected_matches(ExpectExit::Failure, ExitKind::Success));
        assert!(!expected_matches(ExpectExit::Failure, ExitKind::Crash));
    }

    #[test]
    fn any_accepts_crashes_but_not_timeouts() {
        assert!(expected_matches(ExpectExit::Any, ExitKind::Success));
        assert!(expected_matches(ExpectExit::Any, ExitKind::Failure));
        assert!(expected_matches(ExpectExit::Any, ExitKind::Crash));
        assert!(!timeout_matches(ExpectExit::Any));
    }

    #[test]
    fn timeout_expectation_only_matches_timeouts() {
        assert!(timeout_matches(ExpectExit::Timeout));
        assert!(!expected_matches(ExpectExit::Timeout, ExitKind::Success));
        assert!(!timeout_matches(ExpectExit::Success));
    }
}
