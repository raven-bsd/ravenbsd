// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Shrink-only `KNOWN_FAILURES` comparison.

use crate::known::KnownFailures;
use crate::result::{Results, TestResult};

/// The classified outcome of a comparison.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Comparison {
    /// Failing/broken tests not listed in `KNOWN_FAILURES`.
    pub regressions: Vec<TestResult>,
    /// New failures when `--allow-new` is used.
    pub new_known: Vec<TestResult>,
    /// Listed tests that now pass.
    pub fixed: Vec<TestResult>,
    /// Listed failures/skips that behaved as expected.
    pub expected: usize,
}

impl Comparison {
    /// Exit code required by the ticket.
    pub fn exit_code(&self, allow_new: bool) -> i32 {
        if allow_new && !self.new_known.is_empty() {
            4
        } else if !self.regressions.is_empty() {
            1
        } else if !self.fixed.is_empty() {
            3
        } else {
            0
        }
    }
}

/// Compare parsed results with the accepted-failure list.
pub fn compare(results: &Results, known: &KnownFailures, allow_new: bool) -> Comparison {
    let mut comparison = Comparison::default();
    let mut ordered: Vec<&TestResult> = results.results.iter().collect();
    ordered.sort_by(|left, right| {
        left.suite
            .cmp(&right.suite)
            .then_with(|| left.name.cmp(&right.name))
    });
    for result in ordered {
        if result.status.is_failure() {
            if known.contains(&result.suite, &result.name) {
                comparison.expected += 1;
            } else if allow_new {
                comparison.new_known.push(result.clone());
            } else {
                comparison.regressions.push(result.clone());
            }
        } else if result.status == crate::Status::Pass
            && known.contains(&result.suite, &result.name)
        {
            comparison.fixed.push(result.clone());
        } else if known.contains(&result.suite, &result.name) {
            comparison.expected += 1;
        }
    }
    comparison
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::result::Status;

    fn result(name: &str, status: Status) -> TestResult {
        TestResult {
            suite: String::from("suite"),
            name: name.to_string(),
            status,
            duration_ms: None,
            message: None,
        }
    }

    fn results(results: Vec<TestResult>) -> Results {
        Results {
            tool: String::from("x"),
            tool_version: None,
            target: None,
            unparsed: 0,
            results,
        }
    }

    #[test]
    fn classifies_regression_and_fixed() {
        let known = KnownFailures::parse("suite:old\n").unwrap();
        let comparison = compare(
            &results(vec![
                result("new", Status::Fail),
                result("old", Status::Pass),
            ]),
            &known,
            false,
        );
        assert_eq!(comparison.regressions.len(), 1);
        assert_eq!(comparison.fixed.len(), 1);
        assert_eq!(comparison.exit_code(false), 1);
    }

    #[test]
    fn allow_new_returns_four() {
        let known = KnownFailures::parse("").unwrap();
        let comparison = compare(&results(vec![result("new", Status::Fail)]), &known, true);
        assert!(comparison.regressions.is_empty());
        assert_eq!(comparison.new_known.len(), 1);
        assert_eq!(comparison.exit_code(true), 4);
    }
}
