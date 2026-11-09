// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The unified result schema.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// Outcome of one test.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Status {
    /// The test passed.
    Pass,
    /// The test failed.
    Fail,
    /// The test was skipped or marked TODO.
    Skip,
    /// The harness could not run the test.
    Broken,
}

impl Status {
    /// Lowercase name used in JSON and reports.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Pass => "pass",
            Self::Fail => "fail",
            Self::Skip => "skip",
            Self::Broken => "broken",
        }
    }

    /// Whether this status is a failure for `KNOWN_FAILURES` purposes.
    pub const fn is_failure(self) -> bool {
        matches!(self, Self::Fail | Self::Broken)
    }
}

/// One parsed test result.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TestResult {
    /// Test suite or test file.
    pub suite: String,
    /// Test name, unique within the suite after normalization.
    pub name: String,
    /// Outcome.
    pub status: Status,
    /// Duration in milliseconds when the source provides it.
    #[serde(default)]
    pub duration_ms: Option<u64>,
    /// Failure/skip detail or the test description.
    #[serde(default)]
    pub message: Option<String>,
}

/// One tool run's results.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Results {
    /// Tool name (`pjdfstest` or `ltp`).
    pub tool: String,
    /// Tool version when known.
    #[serde(default)]
    pub tool_version: Option<String>,
    /// Target system/name supplied by the caller.
    #[serde(default)]
    pub target: Option<String>,
    /// Number of non-comment lines the parser did not recognize.
    #[serde(default)]
    pub unparsed: usize,
    /// Parsed results.
    pub results: Vec<TestResult>,
}

impl Results {
    /// Sort by suite and name and suffix duplicate names with `#2`, `#3`, ...
    pub fn normalize(&mut self) {
        let mut seen: BTreeMap<(String, String), usize> = BTreeMap::new();
        for result in &mut self.results {
            let key = (result.suite.clone(), result.name.clone());
            let count = seen.entry(key).or_insert(0);
            *count += 1;
            if *count > 1 {
                result.name = format!("{}#{}", result.name, count);
            }
        }
        self.results.sort_by(|left, right| {
            left.suite
                .cmp(&right.suite)
                .then_with(|| left.name.cmp(&right.name))
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn result(suite: &str, name: &str) -> TestResult {
        TestResult {
            suite: suite.to_string(),
            name: name.to_string(),
            status: Status::Pass,
            duration_ms: None,
            message: None,
        }
    }

    #[test]
    fn normalize_sorts_and_suffixes_duplicates() {
        let mut results = Results {
            tool: "x".to_string(),
            tool_version: None,
            target: None,
            unparsed: 0,
            results: vec![
                result("b", "same"),
                result("a", "z"),
                result("a", "same"),
                result("a", "same"),
            ],
        };
        results.normalize();
        let names: Vec<(&str, &str)> = results
            .results
            .iter()
            .map(|result| (result.suite.as_str(), result.name.as_str()))
            .collect();
        assert_eq!(
            names,
            vec![("a", "same"), ("a", "same#2"), ("a", "z"), ("b", "same")]
        );
    }
}
