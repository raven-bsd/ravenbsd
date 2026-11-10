// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host-side oracles that judge artifacts produced during a QEMU run.
//!
//! A case may declare one or more `[[oracle]]` sections. The runner executes the case first
//! and then calls [`analyze`] for every declared oracle. A missing or malformed artifact is a
//! failure with the reason, never a panic.

mod wav;

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};

pub use wav::{Wav, parse_wav};

/// One `[[oracle]]` section from a case file.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OracleSpec {
    /// Oracle kind. Only `wav` is implemented.
    pub kind: String,
    /// Artifact path, relative to the harness working directory or absolute.
    pub path: String,
    /// Minimum duration in milliseconds.
    #[serde(default)]
    pub min_duration_ms: Option<u64>,
    /// Minimum RMS over all samples, normalised to `0.0..=1.0`.
    #[serde(default)]
    pub min_rms: Option<f64>,
    /// Expected dominant frequency in Hz.
    #[serde(default)]
    pub tone_hz: Option<f64>,
    /// Allowed frequency error as a percentage of `tone_hz`.
    #[serde(default)]
    pub tolerance_percent: Option<f64>,
    /// Maximum run of exactly-zero frames after the first non-zero frame.
    #[serde(default)]
    pub max_gap_ms: Option<u64>,
}

impl OracleSpec {
    /// Validate the section's values. Called while parsing the case file.
    pub fn validate(&self) -> Result<()> {
        if self.kind != "wav" {
            bail!("oracle kind must be `wav`, found `{}`", self.kind);
        }
        if self.path.trim().is_empty() {
            bail!("oracle path must not be empty");
        }
        if self
            .min_rms
            .is_some_and(|value| !value.is_finite() || value < 0.0)
        {
            bail!("oracle min_rms must be a finite non-negative number");
        }
        if self
            .tone_hz
            .is_some_and(|value| !value.is_finite() || value <= 0.0)
        {
            bail!("oracle tone_hz must be a finite positive number");
        }
        if self
            .tolerance_percent
            .is_some_and(|value| !value.is_finite() || value <= 0.0)
        {
            bail!("oracle tolerance_percent must be a finite positive number");
        }
        match (self.tone_hz, self.tolerance_percent) {
            (Some(_), None) => bail!("oracle tone_hz requires tolerance_percent"),
            (None, Some(_)) => bail!("oracle tolerance_percent requires tone_hz"),
            _ => {}
        }
        if self.min_duration_ms == Some(0) {
            bail!("oracle min_duration_ms must be greater than zero");
        }
        Ok(())
    }

    fn resolved_path(&self) -> PathBuf {
        PathBuf::from(&self.path)
    }
}

/// One named check inside an [`OracleReport`].
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct OracleCheck {
    /// Check name, for example `min_rms` or `tone_hz`.
    pub check: String,
    /// Whether the check passed.
    pub passed: bool,
    /// Human-readable actual value and threshold.
    pub detail: String,
}

impl OracleCheck {
    fn pass(check: &str, detail: String) -> Self {
        Self {
            check: check.to_string(),
            passed: true,
            detail,
        }
    }

    fn fail(check: &str, detail: String) -> Self {
        Self {
            check: check.to_string(),
            passed: false,
            detail,
        }
    }
}

/// Result of analysing one oracle artifact.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct OracleReport {
    /// Artifact path from the case file.
    pub path: String,
    /// Checks in declaration order.
    pub checks: Vec<OracleCheck>,
    /// File-level error (missing, malformed, unsupported format); `None` when parsed.
    pub error: Option<String>,
}

impl OracleReport {
    /// Whether the file parsed and every check passed.
    pub fn passed(&self) -> bool {
        self.error.is_none() && self.checks.iter().all(|check| check.passed)
    }

    /// Failed checks, including a file-level error as a synthetic `file` check.
    pub fn failures(&self) -> Vec<String> {
        let mut failures = Vec::new();
        if let Some(error) = &self.error {
            failures.push(error.clone());
        }
        for check in &self.checks {
            if !check.passed {
                failures.push(format!("{}: {}", check.check, check.detail));
            }
        }
        failures
    }
}

/// Serialised sidecar file written next to the case result.
#[derive(Debug, Serialize)]
struct OracleFileReport<'a> {
    case: &'a str,
    oracles: &'a [OracleReport],
}

/// Analyse one oracle, returning a report even for a missing or malformed file.
pub fn analyze(spec: &OracleSpec) -> OracleReport {
    let path = spec.resolved_path();
    let bytes = match fs::read(&path) {
        Ok(bytes) => bytes,
        Err(error) => {
            return OracleReport {
                path: spec.path.clone(),
                checks: Vec::new(),
                error: Some(format!("cannot read {}: {error}", path.display())),
            };
        }
    };
    let wav = match parse_wav(&bytes) {
        Ok(wav) => wav,
        Err(error) => {
            return OracleReport {
                path: spec.path.clone(),
                checks: Vec::new(),
                error: Some(format!("malformed {}: {error:#}", path.display())),
            };
        }
    };
    OracleReport {
        path: spec.path.clone(),
        checks: wav::analyze(&wav, spec),
        error: None,
    }
}

/// Write `<case>.oracle.json` in `out_dir` when at least one oracle ran.
///
/// An existing sidecar is removed when the case no longer declares an oracle, so a stale
/// artifact cannot be mistaken for the current run.
pub fn write_reports(out_dir: &Path, case: &str, reports: &[OracleReport]) -> Result<()> {
    let path = out_dir.join(format!("{case}.oracle.json"));
    if reports.is_empty() {
        match fs::remove_file(&path) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => {
                return Err(error).with_context(|| format!("removing stale {}", path.display()));
            }
        }
        return Ok(());
    }
    fs::create_dir_all(out_dir)
        .with_context(|| format!("creating output directory {}", out_dir.display()))?;
    let wrapper = OracleFileReport {
        case,
        oracles: reports,
    };
    let mut json = serde_json::to_string_pretty(&wrapper).context("serialising oracle report")?;
    json.push('\n');
    fs::write(&path, json).with_context(|| format!("writing {}", path.display()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec(kind: &str) -> OracleSpec {
        OracleSpec {
            kind: kind.to_string(),
            path: "artifact.wav".to_string(),
            min_duration_ms: None,
            min_rms: None,
            tone_hz: None,
            tolerance_percent: None,
            max_gap_ms: None,
        }
    }

    #[test]
    fn valid_specs_are_accepted() {
        assert!(spec("wav").validate().is_ok());
        let mut tone = spec("wav");
        tone.tone_hz = Some(440.0);
        tone.tolerance_percent = Some(2.0);
        assert!(tone.validate().is_ok());
    }

    #[test]
    fn invalid_specs_are_rejected() {
        assert!(spec("png").validate().is_err());
        let mut empty = spec("wav");
        empty.path.clear();
        assert!(empty.validate().is_err());
        let mut tone = spec("wav");
        tone.tone_hz = Some(440.0);
        assert!(tone.validate().is_err());
        let mut tolerance = spec("wav");
        tolerance.tolerance_percent = Some(1.0);
        assert!(tolerance.validate().is_err());
        let mut rms = spec("wav");
        rms.min_rms = Some(-1.0);
        assert!(rms.validate().is_err());
        let mut duration = spec("wav");
        duration.min_duration_ms = Some(0);
        assert!(duration.validate().is_err());
    }

    #[test]
    fn missing_file_is_a_report_not_a_panic() {
        let mut missing = spec("wav");
        missing.path = "/definitely/not/a/real/file.wav".to_string();
        let report = analyze(&missing);
        assert!(!report.passed());
        assert!(report.error.is_some());
        assert!(report.checks.is_empty());
    }

    #[test]
    fn stale_sidecar_is_removed_when_no_oracle_runs() {
        let temp = crate::testutil::TempDir::new("oracle-stale");
        let sidecar = temp.path().join("case.oracle.json");
        std::fs::write(&sidecar, b"stale").unwrap();
        write_reports(temp.path(), "case", &[]).unwrap();
        assert!(!sidecar.exists());
    }
}
