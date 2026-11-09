// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Result classification, JSON reports and on-disk artifacts.

use std::fs;
use std::path::Path;

use anyhow::{Context, Result};
use serde::Serialize;

/// Final verdict for one case.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ResultKind {
    /// All expectations matched, no forbidden line appeared and the exit status matched.
    Pass,
    /// The case ran but an expectation, forbid rule or exit-status condition failed.
    Fail,
    /// The case exceeded `timeout_secs` and `timeout` was not the expected outcome.
    Timeout,
    /// The process exited with a status other than 33/35 and `expect_exit` did not accept it.
    Crash,
    /// The harness could not prepare the image or start the process.
    Error,
    /// QEMU or firmware is unavailable, so the case was not executed.
    Skipped,
}

impl ResultKind {
    /// Whether this result counts as success for `run` and `all` (skipped is not a failure).
    pub fn is_success(self) -> bool {
        matches!(self, Self::Pass | Self::Skipped)
    }

    /// Lower-case name used on the command line and in JSON.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Pass => "pass",
            Self::Fail => "fail",
            Self::Timeout => "timeout",
            Self::Crash => "crash",
            Self::Error => "error",
            Self::Skipped => "skipped",
        }
    }
}

/// Machine-readable result written to `<out>/<case>.json`.
#[derive(Debug, Clone, Serialize)]
pub struct CaseReport {
    /// Case name from the case file.
    pub case: String,
    /// Architecture the case actually ran with.
    pub arch: String,
    /// Final verdict.
    pub result: ResultKind,
    /// Wall-clock duration of image preparation plus execution.
    pub duration_ms: u64,
    /// Process exit status, or `None` for timeout/error/skipped cases.
    pub exit_status: Option<i32>,
    /// Regex sources that matched, in expectation order.
    pub matched: Vec<String>,
    /// Regex sources that did not match.
    pub unmatched: Vec<String>,
    /// Serial lines that matched a `[[forbid]]` regex.
    pub forbidden_hits: Vec<String>,
}

/// Everything the CLI needs to report and persist one case.
#[derive(Debug, Clone)]
pub struct CaseOutcome {
    /// Machine-readable result.
    pub report: CaseReport,
    /// Every serial line captured from the process.
    pub lines: Vec<String>,
    /// Human-readable reason for skipped/error/failure results.
    pub detail: Option<String>,
}

/// Write `<out>/<case>.log` and `<out>/<case>.json`, creating the directory if needed.
pub fn write_outcome(outcome: &CaseOutcome, out_dir: &Path) -> Result<()> {
    fs::create_dir_all(out_dir)
        .with_context(|| format!("creating output directory {}", out_dir.display()))?;
    let log_path = out_dir.join(format!("{}.log", outcome.report.case));
    let json_path = out_dir.join(format!("{}.json", outcome.report.case));
    let mut log = outcome.lines.join("\n");
    if !log.is_empty() {
        log.push('\n');
    }
    fs::write(&log_path, log).with_context(|| format!("writing {}", log_path.display()))?;
    let mut json = serde_json::to_string_pretty(&outcome.report).context("serialising report")?;
    json.push('\n');
    fs::write(&json_path, json).with_context(|| format!("writing {}", json_path.display()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::TempDir;

    fn outcome() -> CaseOutcome {
        CaseOutcome {
            report: CaseReport {
                case: "boot-uname".to_string(),
                arch: "x86_64".to_string(),
                result: ResultKind::Pass,
                duration_ms: 42,
                exit_status: Some(33),
                matched: vec!["^hello$".to_string()],
                unmatched: Vec::new(),
                forbidden_hits: Vec::new(),
            },
            lines: vec!["hello".to_string(), "world".to_string()],
            detail: None,
        }
    }

    #[test]
    fn result_names_are_lowercase() {
        assert_eq!(ResultKind::Pass.as_str(), "pass");
        assert_eq!(ResultKind::Timeout.as_str(), "timeout");
        assert_eq!(ResultKind::Skipped.as_str(), "skipped");
    }

    #[test]
    fn skipped_and_pass_count_as_success() {
        assert!(ResultKind::Pass.is_success());
        assert!(ResultKind::Skipped.is_success());
        assert!(!ResultKind::Fail.is_success());
        assert!(!ResultKind::Timeout.is_success());
        assert!(!ResultKind::Crash.is_success());
        assert!(!ResultKind::Error.is_success());
    }

    #[test]
    fn json_contains_every_contract_field() {
        let json = serde_json::to_value(outcome().report).unwrap();
        for key in [
            "case",
            "arch",
            "result",
            "duration_ms",
            "exit_status",
            "matched",
            "unmatched",
            "forbidden_hits",
        ] {
            assert!(json.get(key).is_some(), "missing {key}");
        }
        assert_eq!(json["result"], "pass");
        assert_eq!(json["case"], "boot-uname");
        assert_eq!(json["duration_ms"], 42);
        assert_eq!(json["exit_status"], 33);
    }

    #[test]
    fn exit_status_is_null_when_absent() {
        let mut value = outcome();
        value.report.exit_status = None;
        value.report.result = ResultKind::Timeout;
        let json = serde_json::to_value(value.report).unwrap();
        assert!(json["exit_status"].is_null());
        assert_eq!(json["result"], "timeout");
    }

    #[test]
    fn write_outcome_creates_log_and_json() {
        let temp = TempDir::new("report-write");
        write_outcome(&outcome(), temp.path()).unwrap();
        assert_eq!(temp.read("boot-uname.log"), "hello\nworld\n");
        let json = temp.read("boot-uname.json");
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed["result"], "pass");
    }

    #[test]
    fn empty_log_is_an_empty_file() {
        let temp = TempDir::new("report-empty");
        let mut value = outcome();
        value.lines.clear();
        write_outcome(&value, temp.path()).unwrap();
        assert_eq!(temp.read("boot-uname.log"), "");
    }
}
