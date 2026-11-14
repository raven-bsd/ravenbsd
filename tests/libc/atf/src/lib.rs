// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host-side runner for donor test suites and the ravenlibc G1 conformance gate.
//!
//! The crate speaks the ATF test-program protocol (`-l`, `-r`, `-s`, `-v`), builds donor C
//! programs twice (host oracle and ravenlibc target), records every test body in a stable table,
//! and refuses to hide a regression. Nothing here is linked into a shipped artifact; the runner is
//! a host tool.

#![forbid(unsafe_code)]

use std::collections::BTreeMap;
use std::fmt;

pub mod atf_proto;
pub mod build;
pub mod check;
pub mod exclude;
pub mod gate;
pub mod gen_make;
pub mod gen_meson;
pub mod owners;
pub mod report;
pub mod result;
pub mod runner;
pub mod suite;
pub mod table;

/// Process exit code for success.
pub const EXIT_OK: u8 = 0;
/// Process exit code for a failed check or a failing test.
pub const EXIT_CHECK_FAILED: u8 = 1;
/// Process exit code for usage or I/O errors.
pub const EXIT_USAGE: u8 = 2;
/// Process exit code for an unmet gate.
pub const EXIT_GATE_NOT_MET: u8 = 3;
/// Process exit code for a missing required host tool.
pub const EXIT_NO_TOOL: u8 = 77;

/// Test framework used by a suite.
#[derive(
    Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, serde::Serialize, serde::Deserialize,
)]
#[serde(rename_all = "kebab-case")]
pub enum Framework {
    /// ATF test programs (`-l`/`-r` protocol).
    Atf,
    /// One process per program, verdict from the exit status.
    ExitStatus,
    /// TAP scripts (pjdfstest); execution is pending.
    Tap,
}

impl Framework {
    /// Stable lowercase name used in files.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Atf => "atf",
            Self::ExitStatus => "exit-status",
            Self::Tap => "tap",
        }
    }

    /// Parse a suite file's `framework` value.
    pub fn parse(value: &str) -> Result<Self, String> {
        match value {
            "atf" => Ok(Self::Atf),
            "exit-status" => Ok(Self::ExitStatus),
            "tap" => Ok(Self::Tap),
            other => Err(format!("unknown framework `{other}`")),
        }
    }
}

/// Build and run mode of one suite invocation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Mode {
    /// Host compiler plus host libc: the C original.
    Oracle,
    /// Host compiler plus ravenlibc headers and `libc.a` from a sysroot.
    Target,
}

impl Mode {
    /// Stable lowercase name used in output paths.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Oracle => "oracle",
            Self::Target => "target",
        }
    }
}

/// Run backend: what executes a target binary.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RunBackend {
    /// Build only; every body is `not-run: no-run-backend`.
    None,
    /// Host Linux execution with a Linux `raven-libc-sys` backend (refused when absent).
    HostLinux,
    /// On-target QEMU execution; an interface only in C71.
    Qemu,
}

impl RunBackend {
    /// Parse the `--run-backend` value.
    pub fn parse(value: &str) -> Result<Self, String> {
        match value {
            "none" => Ok(Self::None),
            "host-linux" => Ok(Self::HostLinux),
            "qemu" => Ok(Self::Qemu),
            other => Err(format!("unknown run backend `{other}`")),
        }
    }
}

/// Verdict for one test body.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum Verdict {
    /// The body passed.
    Pass,
    /// The body failed.
    Fail,
    /// The body skipped itself or a requirement was missing.
    Skip,
    /// A manual or oracle exclusion.
    Excluded,
    /// The oracle build or oracle behaviour failed; auto-excluded by 16 3.B.2.3.
    OracleFail,
    /// No run backend produced a verdict.
    NotRun,
}

impl Verdict {
    /// Stable lowercase token used in TSV files.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Pass => "pass",
            Self::Fail => "fail",
            Self::Skip => "skip",
            Self::Excluded => "excluded",
            Self::OracleFail => "oracle-fail",
            Self::NotRun => "not-run",
        }
    }

    /// Parse a verdict token.
    pub fn parse(value: &str) -> Result<Self, String> {
        match value {
            "pass" => Ok(Self::Pass),
            "fail" => Ok(Self::Fail),
            "skip" => Ok(Self::Skip),
            "excluded" => Ok(Self::Excluded),
            "oracle-fail" => Ok(Self::OracleFail),
            "not-run" => Ok(Self::NotRun),
            other => Err(format!("unknown verdict `{other}`")),
        }
    }
}

impl fmt::Display for Verdict {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

/// Identity of one test body: suite, program and case.
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub struct Case {
    /// Suite name (`netbsd-libc`, ...).
    pub suite: String,
    /// Program path relative to the suite root, without `.c`.
    pub program: String,
    /// Test case ident or the synthetic `unknown-bodies`.
    pub name: String,
}

impl Case {
    /// Create a case identity.
    #[must_use]
    pub fn new(
        suite: impl Into<String>,
        program: impl Into<String>,
        name: impl Into<String>,
    ) -> Self {
        Self {
            suite: suite.into(),
            program: program.into(),
            name: name.into(),
        }
    }
}

/// One observed verdict with its reason and owning ticket.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Outcome {
    /// The body this outcome belongs to.
    pub case: Case,
    /// Verdict class.
    pub verdict: Verdict,
    /// Reason with a class token as its first word; empty only for `pass`.
    pub reason: String,
    /// Owning ticket, or `-` for oracle failures and exclusions.
    pub ticket: String,
}

impl Outcome {
    /// Create an outcome.
    #[must_use]
    pub fn new(
        case: Case,
        verdict: Verdict,
        reason: impl Into<String>,
        ticket: impl Into<String>,
    ) -> Self {
        Self {
            case,
            verdict,
            reason: reason.into(),
            ticket: ticket.into(),
        }
    }
}

/// A parsed ATF result file first line.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Expect {
    /// `passed`
    Pass,
    /// `failed: reason`
    Fail(String),
    /// `skipped: reason`
    Skip(String),
    /// `expected_failure: reason`
    XFail(String),
    /// `expected_exit(N): reason`; `None` is the `-1` wildcard.
    ExitCode(Option<i32>, String),
    /// `expected_signal(N): reason`; `None` is the `-1` wildcard.
    Signal(Option<i32>, String),
    /// `expected_death: reason`
    Death(String),
    /// `expected_timeout: reason`
    Timeout(String),
}

impl Expect {
    /// The reason text carried by the directive.
    #[must_use]
    pub fn reason(&self) -> &str {
        match self {
            Self::Pass => "",
            Self::Fail(reason)
            | Self::Skip(reason)
            | Self::XFail(reason)
            | Self::ExitCode(_, reason)
            | Self::Signal(_, reason)
            | Self::Death(reason)
            | Self::Timeout(reason) => reason,
        }
    }
}

/// One test case listed by `program -l`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TpCase {
    /// The `ident` metadata value.
    pub ident: String,
    /// Every metadata key in file order (a sorted map keeps output deterministic).
    pub meta: BTreeMap<String, String>,
}

/// How a process ended.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProcessStatus {
    /// Normal exit with the given status.
    Exited(i32),
    /// Killed by the given signal.
    Signaled(i32),
}

impl ProcessStatus {
    /// The shell-style description used in a missing-result reason.
    #[must_use]
    pub fn describe(self) -> String {
        match self {
            Self::Exited(code) => format!("exit {code}"),
            Self::Signaled(signal) => format!("signal {signal}"),
        }
    }
}

/// Build status of one program.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum BuildStatus {
    /// Built successfully.
    Built,
    /// Build failed with a classified reason.
    Failed(String),
    /// The program was not generated because its source was refused or unparsable.
    Unparsed(String),
    /// A deliberate status from the suite file (`needs-dynamic`, `needs-fs`, ...).
    Needs(String),
}

impl BuildStatus {
    /// The token written to the build-status TSV.
    #[must_use]
    pub fn token(&self) -> &'static str {
        match self {
            Self::Built => "built",
            Self::Failed(_) => "failed",
            Self::Unparsed(_) => "unparsed",
            Self::Needs(_) => "needs",
        }
    }

    /// The reason text, empty for `built`.
    #[must_use]
    pub fn reason(&self) -> &str {
        match self {
            Self::Built => "",
            Self::Failed(reason) | Self::Unparsed(reason) | Self::Needs(reason) => reason,
        }
    }
}

/// Everything one `suite` invocation observed.
#[derive(Clone, Debug)]
pub struct SuiteRun {
    /// Suite name.
    pub suite: String,
    /// Mode of the run.
    pub mode: Mode,
    /// One outcome per body, sorted by case.
    pub outcomes: Vec<Outcome>,
    /// Build status per program.
    pub builds: BTreeMap<String, BuildStatus>,
}

impl SuiteRun {
    /// Sort outcomes by the binding byte order of (suite, program, case).
    pub fn sort(&mut self) {
        self.outcomes
            .sort_by(|left, right| left.case.cmp(&right.case));
    }
}

/// One G1 criterion row.
#[derive(Clone, Debug)]
pub struct G1Row {
    /// Criterion id (`C2`).
    pub id: &'static str,
    /// Human text.
    pub text: &'static str,
    /// Measured rate in percent, `None` when nothing was measured.
    pub measured: Option<f64>,
    /// Threshold in percent.
    pub threshold: f64,
    /// Whether the threshold is met.
    pub met: bool,
    /// Extra note printed after the arrow.
    pub note: String,
}

/// Byte-order sort helper for outcomes.
pub fn sort_outcomes(outcomes: &mut [Outcome]) {
    outcomes.sort_by(|left, right| left.case.cmp(&right.case));
}

/// Match a glob containing only `*` and `?`.
#[must_use]
pub fn glob_match(pattern: &str, text: &str) -> bool {
    glob_bytes(pattern.as_bytes(), text.as_bytes())
}

fn glob_bytes(pattern: &[u8], text: &[u8]) -> bool {
    match pattern.first() {
        None => text.is_empty(),
        Some(b'*') => {
            glob_bytes(&pattern[1..], text) || (!text.is_empty() && glob_bytes(pattern, &text[1..]))
        }
        Some(b'?') => !text.is_empty() && glob_bytes(&pattern[1..], &text[1..]),
        Some(character) => text.first() == Some(character) && glob_bytes(&pattern[1..], &text[1..]),
    }
}
