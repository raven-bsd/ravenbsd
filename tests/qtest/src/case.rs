// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Case-file parsing and validation (contract C6).

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use regex::Regex;
use serde::Deserialize;

use crate::oracle::OracleSpec;

const VALID_ARCHES: &[&str] = &["x86_64", "aarch64", "riscv64"];

/// Whether `arch` is a supported case architecture.
pub fn is_valid_arch(arch: &str) -> bool {
    VALID_ARCHES.contains(&arch)
}

/// What exit status a case expects.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExpectExit {
    /// Guest success: QEMU status 33.
    Success,
    /// Guest failure: QEMU status 35.
    Failure,
    /// Any exit status, including a crash.
    Any,
    /// The case is expected to hit the timeout.
    Timeout,
}

impl ExpectExit {
    /// Parse the case-file spelling.
    pub fn parse(value: &str) -> Result<Self> {
        match value {
            "success" => Ok(Self::Success),
            "failure" => Ok(Self::Failure),
            "any" => Ok(Self::Any),
            "timeout" => Ok(Self::Timeout),
            other => bail!("expect_exit must be success, failure, any or timeout, found `{other}`"),
        }
    }

    /// Case-file spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Success => "success",
            Self::Failure => "failure",
            Self::Any => "any",
            Self::Timeout => "timeout",
        }
    }
}

/// One `[[expect]]` table.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Expect {
    /// Regex searched in each serial line.
    pub regex: String,
    /// Search all lines instead of only the lines after the previous ordered match.
    #[serde(default)]
    pub unordered: bool,
}

/// One `[[forbid]]` table.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Forbid {
    /// Regex that must not match any serial line.
    pub regex: String,
}

/// One `[[step]]` table: a scripted serial interaction.
///
/// A step waits for a line (or the current partial line) and/or writes text to the guest serial
/// input. Steps run in file order while the process is alive; `[[expect]]` matching still runs
/// after the process exits and is not affected by steps.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Step {
    /// Regex that a serial line must match. A line matched by a wait is consumed.
    #[serde(default)]
    pub wait: Option<String>,
    /// Text written to the guest serial input, verbatim (`\n` in TOML is a newline).
    #[serde(default)]
    pub send: Option<String>,
    /// Pause before writing `send`, in milliseconds; default [`DEFAULT_STEP_DELAY_MS`].
    #[serde(default)]
    pub delay_ms: Option<u64>,
    /// How long `wait` may take before the case fails, in seconds; default the case timeout.
    #[serde(default)]
    pub timeout_secs: Option<u64>,
}

/// Default pause before a step's `send`, in milliseconds.
pub const DEFAULT_STEP_DELAY_MS: u64 = 50;

/// Raw case fields, with compiled regexes stripped.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CaseSpec {
    /// Case name; also the log/JSON/image basename.
    pub name: String,
    /// Architecture: `x86_64`, `aarch64` or `riscv64`.
    pub arch: String,
    /// Build profile passed to `cargo xtask image`.
    pub profile: String,
    /// Overall run timeout in seconds.
    pub timeout_secs: u64,
    /// Optional kernel command line passed to the image builder.
    pub boot_cmdline: Option<String>,
    /// Extra QEMU arguments appended after the fixed ones.
    pub qemu_extra: Vec<String>,
    /// Expected exit condition.
    pub expect_exit: ExpectExit,
    /// When set, the case is not executed and reports this reason as `skipped`.
    pub skip: Option<String>,
}

/// A validated case with compiled regexes.
#[derive(Debug, Clone)]
pub struct Case {
    /// Raw fields exposed for the runner.
    pub spec: CaseSpec,
    /// Expectation tables in file order.
    pub expects: Vec<Expect>,
    /// Forbid tables in file order.
    pub forbids: Vec<Forbid>,
    /// Host-side artifact checks run after the process exits.
    pub oracles: Vec<OracleSpec>,
    /// Scripted serial steps in file order.
    pub steps: Vec<Step>,
    expect_regexes: Vec<Regex>,
    forbid_regexes: Vec<Regex>,
    step_regexes: Vec<Option<Regex>>,
}

impl Case {
    /// Parse and validate case TOML.
    pub fn parse(text: &str) -> Result<Self> {
        let raw: RawCase = toml::from_str(text).context("parsing case TOML")?;
        Self::from_raw(raw)
    }

    /// Read, parse and validate a case file.
    pub fn load(path: &Path) -> Result<Self> {
        let text = fs::read_to_string(path)
            .with_context(|| format!("reading case file {}", path.display()))?;
        Self::parse(&text).with_context(|| format!("validating case file {}", path.display()))
    }

    /// Regexes for [`Self::expects`], same order.
    pub(crate) fn expect_regexes(&self) -> &[Regex] {
        &self.expect_regexes
    }

    /// Regexes for [`Self::forbids`], same order.
    pub(crate) fn forbid_regexes(&self) -> &[Regex] {
        &self.forbid_regexes
    }

    /// Compiled `wait` regexes for [`Self::steps`], same order; `None` for a send-only step.
    pub(crate) fn step_regexes(&self) -> &[Option<Regex>] {
        &self.step_regexes
    }

    fn from_raw(raw: RawCase) -> Result<Self> {
        validate_name(&raw.name)?;
        if !VALID_ARCHES.contains(&raw.arch.as_str()) {
            bail!(
                "arch must be one of x86_64, aarch64, riscv64, found `{}`",
                raw.arch
            );
        }
        if raw.profile.trim().is_empty() {
            bail!("profile must not be empty");
        }
        if raw.timeout_secs == 0 {
            bail!("timeout_secs must be greater than zero");
        }
        let expect_exit = ExpectExit::parse(&raw.expect_exit)?;
        let mut expect_regexes = Vec::with_capacity(raw.expect.len());
        for (index, expect) in raw.expect.iter().enumerate() {
            let regex = Regex::new(&expect.regex)
                .with_context(|| format!("expect[{index}] has an invalid regex"))?;
            expect_regexes.push(regex);
        }
        let mut forbid_regexes = Vec::with_capacity(raw.forbid.len());
        for (index, forbid) in raw.forbid.iter().enumerate() {
            let regex = Regex::new(&forbid.regex)
                .with_context(|| format!("forbid[{index}] has an invalid regex"))?;
            forbid_regexes.push(regex);
        }
        for (index, oracle) in raw.oracle.iter().enumerate() {
            oracle
                .validate()
                .with_context(|| format!("oracle[{index}] is invalid"))?;
        }
        let timeout_ms = raw.timeout_secs.saturating_mul(1000);
        let mut step_regexes = Vec::with_capacity(raw.step.len());
        for (index, step) in raw.step.iter().enumerate() {
            if step.wait.is_none() && step.send.is_none() {
                bail!("step[{index}] needs wait, send or both");
            }
            let regex = match &step.wait {
                Some(pattern) => Some(
                    Regex::new(pattern)
                        .with_context(|| format!("step[{index}] has an invalid wait regex"))?,
                ),
                None => None,
            };
            if step.timeout_secs == Some(0) {
                bail!("step[{index}] timeout_secs must be greater than zero");
            }
            if step
                .timeout_secs
                .is_some_and(|secs| secs > raw.timeout_secs)
            {
                bail!("step[{index}] timeout_secs must not exceed the case timeout");
            }
            if let Some(delay) = step.delay_ms
                && delay >= timeout_ms
            {
                bail!("step[{index}] delay_ms must be shorter than the case timeout");
            }
            step_regexes.push(regex);
        }
        if let Some(skip) = &raw.skip
            && skip.trim().is_empty()
        {
            bail!("skip needs a reason");
        }
        Ok(Self {
            spec: CaseSpec {
                name: raw.name,
                arch: raw.arch,
                profile: raw.profile,
                timeout_secs: raw.timeout_secs,
                boot_cmdline: raw.boot_cmdline,
                qemu_extra: raw.qemu_extra,
                expect_exit,
                skip: raw.skip,
            },
            expects: raw.expect,
            forbids: raw.forbid,
            oracles: raw.oracle,
            steps: raw.step,
            expect_regexes,
            forbid_regexes,
            step_regexes,
        })
    }
}

fn validate_name(name: &str) -> Result<()> {
    if name.is_empty() {
        bail!("name must not be empty");
    }
    if name == "." || name == ".." {
        bail!("name must not be `.` or `..`");
    }
    if name.contains('/') || name.contains('\\') {
        bail!("name must not contain path separators");
    }
    Ok(())
}

/// List `.toml` case files in `dir`, sorted by path. A missing directory is an empty list.
pub fn list_case_files(dir: &Path) -> Result<Vec<PathBuf>> {
    if !dir.is_dir() {
        return Ok(Vec::new());
    }
    let mut files = Vec::new();
    for entry in
        fs::read_dir(dir).with_context(|| format!("reading case directory {}", dir.display()))?
    {
        let entry = entry?;
        let path = entry.path();
        if path.extension().and_then(|ext| ext.to_str()) == Some("toml") && path.is_file() {
            files.push(path);
        }
    }
    files.sort();
    Ok(files)
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawCase {
    name: String,
    arch: String,
    profile: String,
    timeout_secs: u64,
    #[serde(default)]
    boot_cmdline: Option<String>,
    #[serde(default)]
    qemu_extra: Vec<String>,
    expect_exit: String,
    #[serde(default)]
    skip: Option<String>,
    #[serde(default, rename = "expect")]
    expect: Vec<Expect>,
    #[serde(default, rename = "forbid")]
    forbid: Vec<Forbid>,
    #[serde(default, rename = "oracle")]
    oracle: Vec<OracleSpec>,
    #[serde(default, rename = "step")]
    step: Vec<Step>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::TempDir;

    fn minimal() -> &'static str {
        "name = \"smoke\"\narch = \"x86_64\"\nprofile = \"base\"\ntimeout_secs = 10\nexpect_exit = \"success\"\n"
    }

    #[test]
    fn minimal_case_parses() {
        let case = Case::parse(minimal()).unwrap();
        assert_eq!(case.spec.name, "smoke");
        assert_eq!(case.spec.arch, "x86_64");
        assert_eq!(case.spec.timeout_secs, 10);
        assert_eq!(case.spec.expect_exit, ExpectExit::Success);
        assert!(case.spec.boot_cmdline.is_none());
        assert!(case.spec.qemu_extra.is_empty());
        assert!(case.expects.is_empty());
        assert!(case.forbids.is_empty());
    }

    #[test]
    fn contract_example_parses() {
        let text = r#"
name = "boot-uname"
arch = "x86_64"
profile = "base"
timeout_secs = 20
boot_cmdline = "console=ttyS0 test-exit=1"
qemu_extra = []
expect_exit = "success"
[[expect]]
regex = '^RavenBSD kernel 0\.1 \(x86_64\)'
[[expect]]
regex = '^RavenBSD \S+ 0\.1 RavenBSD 0\.1 \(\S+\) #\d+: .+ x86_64$'
[[forbid]]
regex = '(?i)panic'
"#;
        let case = Case::parse(text).unwrap();
        assert_eq!(case.expects.len(), 2);
        assert!(!case.expects[0].unordered);
        assert_eq!(case.forbids.len(), 1);
        assert_eq!(case.forbids[0].regex, "(?i)panic");
    }

    #[test]
    fn unknown_key_is_rejected() {
        let text = format!("{}mystery = 1\n", minimal());
        let error = format!("{:#}", Case::parse(&text).unwrap_err());
        assert!(error.contains("unknown field"), "{error}");
    }

    #[test]
    fn missing_required_field_is_rejected() {
        let text = minimal().replace("name = \"smoke\"\n", "");
        let error = format!("{:#}", Case::parse(&text).unwrap_err());
        assert!(error.contains("name"), "{error}");
    }

    #[test]
    fn bad_arch_is_rejected() {
        let text = minimal().replace("x86_64", "sparc64");
        let error = Case::parse(&text).unwrap_err().to_string();
        assert!(error.contains("arch"), "{error}");
    }

    #[test]
    fn bad_expect_exit_is_rejected() {
        let text = minimal().replace("success", "maybe");
        let error = Case::parse(&text).unwrap_err().to_string();
        assert!(error.contains("expect_exit"), "{error}");
    }

    #[test]
    fn zero_timeout_is_rejected() {
        let text = minimal().replace("timeout_secs = 10", "timeout_secs = 0");
        let error = Case::parse(&text).unwrap_err().to_string();
        assert!(error.contains("timeout_secs"), "{error}");
    }

    #[test]
    fn invalid_expect_regex_is_rejected() {
        let text = format!("{}[[expect]]\nregex = \"(unclosed\"\n", minimal());
        let error = Case::parse(&text).unwrap_err().to_string();
        assert!(error.contains("expect[0]"), "{error}");
    }

    #[test]
    fn invalid_forbid_regex_is_rejected() {
        let text = format!("{}[[forbid]]\nregex = \"[bad\"\n", minimal());
        let error = Case::parse(&text).unwrap_err().to_string();
        assert!(error.contains("forbid[0]"), "{error}");
    }

    #[test]
    fn path_separator_in_name_is_rejected() {
        let text = minimal().replace("smoke", "../escape");
        let error = Case::parse(&text).unwrap_err().to_string();
        assert!(error.contains("path separators"), "{error}");
    }

    #[test]
    fn unordered_is_optional_and_false_by_default() {
        let text = format!(
            "{}[[expect]]\nregex = \"a\"\n[[expect]]\nregex = \"b\"\nunordered = true\n",
            minimal()
        );
        let case = Case::parse(&text).unwrap();
        assert!(!case.expects[0].unordered);
        assert!(case.expects[1].unordered);
    }

    #[test]
    fn unknown_expect_key_is_rejected() {
        let text = format!("{}[[expect]]\nregex = \"a\"\nextra = 1\n", minimal());
        let error = format!("{:#}", Case::parse(&text).unwrap_err());
        assert!(error.contains("unknown field"), "{error}");
    }

    #[test]
    fn list_case_files_sorts_and_filters() {
        let temp = TempDir::new("case-list");
        temp.write("cases/b.toml", minimal());
        temp.write("cases/a.toml", minimal());
        temp.write("cases/README.md", "# docs\n");
        let files = list_case_files(&temp.path().join("cases")).unwrap();
        assert_eq!(files.len(), 2);
        assert!(files[0].ends_with("a.toml"));
        assert!(files[1].ends_with("b.toml"));
    }

    #[test]
    fn missing_case_directory_is_empty() {
        let temp = TempDir::new("case-missing");
        assert!(
            list_case_files(&temp.path().join("nope"))
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn load_reports_the_file_name_on_error() {
        let temp = TempDir::new("case-load");
        temp.write("bad.toml", "name = 1\n");
        let error = Case::load(&temp.path().join("bad.toml"))
            .unwrap_err()
            .to_string();
        assert!(error.contains("bad.toml"), "{error}");
    }

    #[test]
    fn step_section_parses_with_defaults() {
        let text = format!(
            "{}[[step]]\nwait = '^ready> $'\nsend = \"uname\\n\"\n",
            minimal()
        );
        let case = Case::parse(&text).unwrap();
        assert_eq!(case.steps.len(), 1);
        assert_eq!(case.steps[0].wait.as_deref(), Some("^ready> $"));
        assert_eq!(case.steps[0].send.as_deref(), Some("uname\n"));
        assert_eq!(case.steps[0].delay_ms, None);
        assert_eq!(case.steps[0].timeout_secs, None);
        let regexes = case.step_regexes();
        assert!(regexes[0].as_ref().unwrap().is_match("ready> "));
    }

    #[test]
    fn steps_keep_file_order_between_expects() {
        let text = format!(
            "{}[[step]]\nwait = \"^one$\"\n[[expect]]\nregex = '^x$'\n\
             [[step]]\nsend = \"two\\n\"\n[[step]]\nwait = \"^three$\"\n",
            minimal()
        );
        let case = Case::parse(&text).unwrap();
        assert_eq!(case.expects.len(), 1);
        let waits: Vec<&str> = case
            .steps
            .iter()
            .map(|step| step.wait.as_deref().unwrap_or("<send>"))
            .collect();
        assert_eq!(waits, vec!["^one$", "<send>", "^three$"]);
    }

    #[test]
    fn a_step_needs_wait_send_or_both() {
        let text = format!("{}[[step]]\ndelay_ms = 10\n", minimal());
        let error = format!("{:#}", Case::parse(&text).unwrap_err());
        assert!(
            error.contains("step[0] needs wait, send or both"),
            "{error}"
        );
    }

    #[test]
    fn invalid_step_wait_regex_is_rejected() {
        let text = format!("{}[[step]]\nwait = \"(unclosed\"\n", minimal());
        let error = format!("{:#}", Case::parse(&text).unwrap_err());
        assert!(error.contains("step[0]"), "{error}");
        assert!(error.contains("wait regex"), "{error}");
    }

    #[test]
    fn unknown_step_key_is_rejected() {
        let text = format!("{}[[step]]\nwait = \"a\"\nmystery = 1\n", minimal());
        let error = format!("{:#}", Case::parse(&text).unwrap_err());
        assert!(error.contains("unknown field"), "{error}");
    }

    #[test]
    fn step_timeout_bounds_are_enforced() {
        let text = format!("{}[[step]]\nwait = \"a\"\ntimeout_secs = 0\n", minimal());
        let error = format!("{:#}", Case::parse(&text).unwrap_err());
        assert!(error.contains("timeout_secs"), "{error}");

        let text = format!("{}[[step]]\nwait = \"a\"\ntimeout_secs = 11\n", minimal());
        let error = format!("{:#}", Case::parse(&text).unwrap_err());
        assert!(error.contains("case timeout"), "{error}");

        let text = format!("{}[[step]]\nwait = \"a\"\ntimeout_secs = 10\n", minimal());
        assert!(Case::parse(&text).is_ok());
    }

    #[test]
    fn step_delay_must_be_shorter_than_the_case_timeout() {
        let text = format!("{}[[step]]\nsend = \"x\"\ndelay_ms = 10000\n", minimal());
        let error = format!("{:#}", Case::parse(&text).unwrap_err());
        assert!(error.contains("delay_ms"), "{error}");
        let text = format!("{}[[step]]\nsend = \"x\"\ndelay_ms = 9999\n", minimal());
        assert!(Case::parse(&text).is_ok());
    }

    #[test]
    fn skip_is_optional_and_needs_a_reason() {
        assert_eq!(Case::parse(minimal()).unwrap().spec.skip, None);
        let text = format!("{}skip = \"needs S27\"\n", minimal());
        assert_eq!(
            Case::parse(&text).unwrap().spec.skip.as_deref(),
            Some("needs S27")
        );
        let text = format!("{}skip = \"  \"\n", minimal());
        let error = format!("{:#}", Case::parse(&text).unwrap_err());
        assert!(error.contains("skip needs a reason"), "{error}");
    }

    #[test]
    fn a_case_without_steps_has_none() {
        let case = Case::parse(minimal()).unwrap();
        assert!(case.steps.is_empty());
        assert!(case.step_regexes().is_empty());
    }
}
