// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Case execution: image preparation, process launch, matching and result classification.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

use crate::case::Case;
use crate::dialogue::Dialogue;
use crate::exitmap::{self, ExitKind};
use crate::matcher;
use crate::oracle::{self, OracleReport};
use crate::qemu;
use crate::report::{CaseOutcome, CaseReport, ResultKind};
use crate::runner::{self, RunSpec};

/// Options controlling one case execution.
#[derive(Debug, Clone)]
pub struct ExecuteOptions {
    /// Override the case file's `arch`.
    pub arch: Option<String>,
    /// Use this image instead of building one.
    pub image: Option<PathBuf>,
    /// Directory for the log, JSON result, built image and per-run firmware vars.
    pub out_dir: PathBuf,
    /// OVMF/AAVMF code pflash path; overrides the environment.
    pub ovmf_code: Option<PathBuf>,
    /// OVMF/AAVMF vars pflash path; overrides the environment.
    pub ovmf_vars: Option<PathBuf>,
    /// Replace QEMU with this command vector (tests and local dry runs).
    pub runner: Option<Vec<String>>,
    /// Do not invoke `cargo xtask image`; require the image to exist already.
    pub no_build: bool,
}

impl ExecuteOptions {
    /// Defaults: `build/qtest` output, case arch, no overrides.
    pub fn new() -> Self {
        Self {
            arch: None,
            image: None,
            out_dir: PathBuf::from("build/qtest"),
            ovmf_code: None,
            ovmf_vars: None,
            runner: None,
            no_build: false,
        }
    }
}

impl Default for ExecuteOptions {
    fn default() -> Self {
        Self::new()
    }
}

enum PrepareError {
    Skipped(String),
    Error(String),
}

/// Everything produced by one case execution, including host-side oracle reports.
#[derive(Debug, Clone)]
pub struct Executed {
    /// Serial matching, exit status and final verdict.
    pub outcome: CaseOutcome,
    /// Oracle reports in case-file order; empty when the case declares none.
    pub oracles: Vec<OracleReport>,
}

/// Execute one case and return its outcome. Harness errors become `Error` results.
pub fn execute(case: &Case, options: &ExecuteOptions) -> CaseOutcome {
    execute_with_oracles(case, options).outcome
}

/// Execute one case, then run every declared host-side oracle.
pub fn execute_with_oracles(case: &Case, options: &ExecuteOptions) -> Executed {
    let mut outcome = run_case(case, options);
    let mut oracles = Vec::new();
    if matches!(
        outcome.report.result,
        ResultKind::Pass | ResultKind::Fail | ResultKind::Timeout | ResultKind::Crash
    ) {
        for spec in &case.oracles {
            oracles.push(oracle::analyze(spec));
        }
    }
    if oracles.iter().any(|report| !report.passed()) {
        let mut reasons = Vec::new();
        for report in &oracles {
            for failure in report.failures() {
                reasons.push(format!("oracle {}: {failure}", report.path));
            }
        }
        if outcome.report.result == ResultKind::Pass {
            outcome.report.result = ResultKind::Fail;
        }
        let reason = reasons.join("; ");
        outcome.detail = Some(match outcome.detail {
            Some(existing) => format!("{existing}; {reason}"),
            None => reason,
        });
    }
    Executed { outcome, oracles }
}

fn run_case(case: &Case, options: &ExecuteOptions) -> CaseOutcome {
    let started = Instant::now();
    let arch = options
        .arch
        .clone()
        .unwrap_or_else(|| case.spec.arch.clone());
    let report = CaseReport {
        case: case.spec.name.clone(),
        arch,
        result: ResultKind::Error,
        duration_ms: 0,
        exit_status: None,
        matched: Vec::new(),
        unmatched: Vec::new(),
        forbidden_hits: Vec::new(),
    };
    let arch = report.arch.clone();

    if let Some(reason) = &case.spec.skip {
        return finish(
            report,
            Vec::new(),
            Some(reason.clone()),
            ResultKind::Skipped,
            started,
        );
    }

    if let Err(reason) = run_pre_commands(case, &arch) {
        return finish(report, Vec::new(), Some(reason), ResultKind::Error, started);
    }

    let (program, args) = match &options.runner {
        Some(runner) if runner.is_empty() => {
            return finish(
                report,
                Vec::new(),
                Some("--runner needs a command".to_string()),
                ResultKind::Error,
                started,
            );
        }
        Some(runner) => (runner[0].clone(), runner[1..].to_vec()),
        None => match prepare_external(case, &arch, options) {
            Ok(command) => command,
            Err(PrepareError::Skipped(reason)) => {
                return finish(
                    report,
                    Vec::new(),
                    Some(reason),
                    ResultKind::Skipped,
                    started,
                );
            }
            Err(PrepareError::Error(reason)) => {
                return finish(report, Vec::new(), Some(reason), ResultKind::Error, started);
            }
        },
    };

    let timeout = Duration::from_secs(case.spec.timeout_secs);
    let spec = RunSpec {
        program,
        args,
        timeout,
    };
    let mut dialogue = Dialogue::from_case(case);
    let run = match dialogue.as_mut() {
        Some(active) => runner::run_dialogue(&spec, active),
        None => runner::run(&spec),
    };
    let run = match run {
        Ok(run) => run,
        Err(error) => {
            return finish(
                report,
                Vec::new(),
                Some(format!("could not start case: {error:#}")),
                ResultKind::Error,
                started,
            );
        }
    };

    let exit = exitmap::classify(run.status);
    let matches = matcher::evaluate(case, &run.lines);
    let mut report = report;
    report.exit_status = run.status;
    report.matched = matches.matched;
    report.unmatched = matches.unmatched;
    report.forbidden_hits = matches.forbidden_hits;

    let exit_ok = if run.timed_out {
        exitmap::timeout_matches(case.spec.expect_exit)
    } else {
        exitmap::expected_matches(case.spec.expect_exit, exit)
    };
    let passed = exit_ok && report.unmatched.is_empty() && report.forbidden_hits.is_empty();
    let step_failure = run.step_failure;
    let result = if step_failure.is_some() {
        ResultKind::Fail
    } else if passed {
        ResultKind::Pass
    } else if run.timed_out {
        ResultKind::Timeout
    } else if exit == ExitKind::Crash {
        ResultKind::Crash
    } else {
        ResultKind::Fail
    };
    let detail = match &step_failure {
        Some(failure) => Some(describe_step_failure(failure)),
        None if passed => None,
        None => Some(describe_failure(case, &report, run.timed_out, exit)),
    };
    finish(report, run.lines, detail, result, started)
}

fn prepare_external(
    case: &Case,
    arch: &str,
    options: &ExecuteOptions,
) -> Result<(String, Vec<String>), PrepareError> {
    let image = prepare_image(case, arch, options)?;
    let firmware = qemu::resolve_firmware(
        arch,
        options.ovmf_code.as_deref(),
        options.ovmf_vars.as_deref(),
    )
    .map_err(PrepareError::Skipped)?;
    let program = qemu::program_for(arch).map_err(PrepareError::Skipped)?;
    if !qemu::command_exists(&program) {
        return Err(PrepareError::Skipped(format!(
            "QEMU binary not found in PATH: {program}"
        )));
    }
    fs::create_dir_all(&options.out_dir).map_err(|error| {
        PrepareError::Error(format!(
            "cannot create output directory {}: {error}",
            options.out_dir.display()
        ))
    })?;
    let vars = options.out_dir.join(format!("{}.vars.fd", case.spec.name));
    fs::copy(&firmware.vars, &vars).map_err(|error| {
        PrepareError::Error(format!(
            "cannot copy firmware vars {} to {}: {error}",
            firmware.vars.display(),
            vars.display()
        ))
    })?;
    let firmware = qemu::Firmware {
        code: firmware.code,
        vars,
    };
    let command = qemu::qemu_command(arch, &image, &firmware, &case.spec.qemu_extra)
        .map_err(PrepareError::Error)?;
    Ok((command.program, command.args))
}

fn prepare_image(
    case: &Case,
    arch: &str,
    options: &ExecuteOptions,
) -> Result<PathBuf, PrepareError> {
    if let Some(image) = &options.image {
        if !image.is_file() {
            return Err(PrepareError::Error(format!(
                "image not found: {}",
                image.display()
            )));
        }
        return Ok(image.clone());
    }
    fs::create_dir_all(&options.out_dir).map_err(|error| {
        PrepareError::Error(format!(
            "cannot create output directory {}: {error}",
            options.out_dir.display()
        ))
    })?;
    let image = options.out_dir.join(format!("{}.img", case.spec.name));
    if options.no_build {
        if !image.is_file() {
            return Err(PrepareError::Error(format!(
                "image not found (--no-build): {}",
                image.display()
            )));
        }
        return Ok(image);
    }
    let (program, args) = qemu::image_command(
        arch,
        &case.spec.profile,
        case.spec.boot_cmdline.as_deref(),
        case.spec.initfs.as_deref().map(Path::new),
        case.spec.loader_kernel.as_deref().map(Path::new),
        &image,
    );
    let output = Command::new(&program)
        .args(&args)
        .output()
        .map_err(|error| PrepareError::Error(format!("cannot run `{program}`: {error}")))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let message = stderr
            .lines()
            .rev()
            .find(|line| !line.trim().is_empty())
            .unwrap_or("no stderr output")
            .trim();
        return Err(PrepareError::Error(format!(
            "image build failed ({}): {message}",
            output.status
        )));
    }
    if !image.is_file() {
        return Err(PrepareError::Error(format!(
            "image build produced no file at {}",
            image.display()
        )));
    }
    Ok(image)
}

fn last_line(text: &str) -> Option<&str> {
    text.lines().rev().find(|line| !line.trim().is_empty())
}

/// Run the case's `pre` commands in order with `sh -c`.
///
/// They run from the harness working directory (the repository root for `cargo xtask qtest`)
/// and before the image build. A non-zero exit turns the case into `error`.
fn run_pre_commands(case: &Case, arch: &str) -> Result<(), String> {
    for (index, command) in case.spec.pre.iter().enumerate() {
        let output = Command::new("sh")
            .arg("-c")
            .arg(command)
            .env("RAVEN_QT_ARCH", arch)
            .output()
            .map_err(|error| format!("pre[{index}] `{command}` could not start: {error}"))?;
        if output.status.success() {
            continue;
        }
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stdout = String::from_utf8_lossy(&output.stdout);
        let message = last_line(&stderr)
            .or_else(|| last_line(&stdout))
            .unwrap_or("no output");
        return Err(format!(
            "pre[{index}] `{command}` failed with {}: {message}",
            output.status
        ));
    }
    Ok(())
}

fn describe_failure(case: &Case, report: &CaseReport, timed_out: bool, exit: ExitKind) -> String {
    let mut parts = Vec::new();
    if timed_out {
        parts.push(format!(
            "timed out after {} s (expected {})",
            case.spec.timeout_secs,
            case.spec.expect_exit.as_str()
        ));
    } else {
        match report.exit_status {
            Some(status) => parts.push(format!("exit status {status}")),
            None => parts.push("process ended without an exit status".to_string()),
        }
        if !exitmap::expected_matches(case.spec.expect_exit, exit) {
            parts.push(format!("expected {}", case.spec.expect_exit.as_str()));
        }
    }
    if !report.unmatched.is_empty() {
        parts.push(format!(
            "unmatched expectations: {}",
            report.unmatched.join(", ")
        ));
    }
    if !report.forbidden_hits.is_empty() {
        parts.push(format!(
            "{} forbidden line(s) matched",
            report.forbidden_hits.len()
        ));
    }
    parts.join("; ")
}

fn describe_step_failure(failure: &runner::StepFailure) -> String {
    let mut text = format!(
        "step {} ({}) failed: {}",
        failure.index + 1,
        failure.pattern,
        failure.reason
    );
    if failure.lines.is_empty() {
        text.push_str("\nno serial output before the failure");
    } else {
        text.push_str(&format!("\nlast {} serial line(s):", failure.lines.len()));
        for line in &failure.lines {
            text.push('\n');
            text.push_str(line);
        }
    }
    text
}

fn finish(
    mut report: CaseReport,
    lines: Vec<String>,
    detail: Option<String>,
    result: ResultKind,
    started: Instant,
) -> CaseOutcome {
    report.result = result;
    report.duration_ms = u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX);
    CaseOutcome {
        report,
        lines,
        detail,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::TempDir;

    fn case(timeout: u64, expect_exit: &str, expects: &str) -> Case {
        let text = format!(
            "name = \"unit\"\narch = \"x86_64\"\nprofile = \"base\"\ntimeout_secs = {timeout}\n\
             expect_exit = \"{expect_exit}\"\n{expects}"
        );
        Case::parse(&text).unwrap()
    }

    fn runner(script: &str) -> ExecuteOptions {
        let mut options = ExecuteOptions::new();
        options.runner = Some(vec!["sh".to_string(), "-c".to_string(), script.to_string()]);
        options
    }

    #[test]
    fn runner_pass() {
        let case = case(5, "success", "[[expect]]\nregex = \"^hello$\"\n");
        let outcome = execute(&case, &runner("printf 'hello\\n'; exit 33"));
        assert_eq!(outcome.report.result, ResultKind::Pass);
        assert_eq!(outcome.report.exit_status, Some(33));
        assert_eq!(outcome.report.matched, vec!["^hello$"]);
        assert!(outcome.detail.is_none());
    }

    #[test]
    fn unmatched_expectation_fails() {
        let case = case(5, "success", "[[expect]]\nregex = \"^missing$\"\n");
        let outcome = execute(&case, &runner("printf 'other\\n'; exit 33"));
        assert_eq!(outcome.report.result, ResultKind::Fail);
        assert_eq!(outcome.report.unmatched, vec!["^missing$"]);
        assert!(outcome.detail.unwrap().contains("unmatched"));
    }

    #[test]
    fn expected_failure_passes() {
        let case = case(5, "failure", "");
        let outcome = execute(&case, &runner("exit 35"));
        assert_eq!(outcome.report.result, ResultKind::Pass);
        assert_eq!(outcome.report.exit_status, Some(35));
    }

    #[test]
    fn expected_timeout_passes() {
        let case = case(1, "timeout", "");
        let outcome = execute(&case, &runner("sleep 5"));
        assert_eq!(outcome.report.result, ResultKind::Pass);
        assert!(outcome.report.duration_ms < 3000);
    }

    #[test]
    fn unexpected_timeout_is_reported() {
        let case = case(1, "success", "");
        let outcome = execute(&case, &runner("sleep 5"));
        assert_eq!(outcome.report.result, ResultKind::Timeout);
        assert!(outcome.detail.unwrap().contains("timed out"));
    }

    #[test]
    fn crash_is_reported() {
        let case = case(5, "success", "");
        let outcome = execute(&case, &runner("exit 7"));
        assert_eq!(outcome.report.result, ResultKind::Crash);
        assert_eq!(outcome.report.exit_status, Some(7));
    }

    #[test]
    fn any_accepts_a_crash() {
        let case = case(5, "any", "");
        let outcome = execute(&case, &runner("exit 7"));
        assert_eq!(outcome.report.result, ResultKind::Pass);
    }

    #[test]
    fn forbid_hit_fails() {
        let case = case(5, "success", "[[forbid]]\nregex = \"panic\"\n");
        let outcome = execute(&case, &runner("printf 'panic\\n'; exit 33"));
        assert_eq!(outcome.report.result, ResultKind::Fail);
        assert_eq!(outcome.report.forbidden_hits, vec!["panic"]);
    }

    #[test]
    fn missing_firmware_skips() {
        let temp = TempDir::new("execute-skip");
        let image = temp.path().join("dummy.img");
        std::fs::write(&image, b"image").unwrap();
        let case = Case::parse(
            "name = \"skip\"\narch = \"riscv64\"\nprofile = \"base\"\ntimeout_secs = 5\n\
             expect_exit = \"success\"\n",
        )
        .unwrap();
        let mut options = ExecuteOptions::new();
        options.image = Some(image);
        let outcome = execute(&case, &options);
        assert_eq!(outcome.report.result, ResultKind::Skipped);
        assert!(outcome.detail.unwrap().contains("firmware"));
    }

    #[test]
    fn missing_prebuilt_image_is_an_error() {
        let temp = TempDir::new("execute-nobuild");
        let case = Case::parse(
            "name = \"nobuild\"\narch = \"x86_64\"\nprofile = \"base\"\ntimeout_secs = 5\n\
             expect_exit = \"success\"\n",
        )
        .unwrap();
        let mut options = ExecuteOptions::new();
        options.out_dir = temp.path().to_path_buf();
        options.no_build = true;
        let outcome = execute(&case, &options);
        assert_eq!(outcome.report.result, ResultKind::Error);
        assert!(outcome.detail.unwrap().contains("--no-build"));
    }

    #[test]
    fn missing_explicit_image_is_an_error() {
        let temp = TempDir::new("execute-image");
        let case = Case::parse(
            "name = \"image\"\narch = \"x86_64\"\nprofile = \"base\"\ntimeout_secs = 5\n\
             expect_exit = \"success\"\n",
        )
        .unwrap();
        let mut options = ExecuteOptions::new();
        options.image = Some(temp.path().join("missing.img"));
        let outcome = execute(&case, &options);
        assert_eq!(outcome.report.result, ResultKind::Error);
        assert!(outcome.detail.unwrap().contains("image not found"));
    }

    #[test]
    fn arch_override_is_recorded() {
        let case = case(5, "success", "");
        let outcome = execute(&case, &runner("exit 33"));
        assert_eq!(outcome.report.arch, "x86_64");
        let mut options = runner("exit 33");
        options.arch = Some("aarch64".to_string());
        let outcome = execute(&case, &options);
        assert_eq!(outcome.report.arch, "aarch64");
        assert_eq!(outcome.report.result, ResultKind::Pass);
    }

    #[test]
    fn a_skipped_case_does_not_run() {
        let text = "name = \"skipped\"\narch = \"x86_64\"\nprofile = \"base\"\n\
                    timeout_secs = 5\nexpect_exit = \"success\"\n\
                    skip = \"needs S27 (DEBUG_READ)\"\n";
        let case = Case::parse(text).unwrap();
        let outcome = execute(&case, &runner("printf 'must not run\\n'; exit 1"));
        assert_eq!(outcome.report.result, ResultKind::Skipped);
        assert!(outcome.lines.is_empty());
        assert!(outcome.detail.unwrap().contains("S27"));
    }

    #[test]
    fn steps_run_against_the_fake_runner() {
        let case = case(
            5,
            "success",
            "[[expect]]\nregex = \"^got: uname$\"\n\
             [[step]]\nwait = \"^ready$\"\nsend = \"uname\\n\"\ndelay_ms = 0\n",
        );
        let outcome = execute(
            &case,
            &runner("printf 'ready\\n'; read line; printf 'got: %s\\n' \"$line\"; exit 33"),
        );
        assert_eq!(
            outcome.report.result,
            ResultKind::Pass,
            "{:?}",
            outcome.detail
        );
    }

    #[test]
    fn a_step_failure_fails_the_case_and_reports_the_step() {
        let case = case(
            5,
            "success",
            "[[step]]\nwait = \"^never$\"\ntimeout_secs = 1\n",
        );
        let outcome = execute(&case, &runner("printf 'started\\n'; sleep 5"));
        assert_eq!(outcome.report.result, ResultKind::Fail);
        let detail = outcome.detail.expect("detail");
        assert!(detail.contains("step 1"), "{detail}");
        assert!(detail.contains("^never$"), "{detail}");
        assert!(detail.contains("started"), "{detail}");
    }

    #[test]
    fn pre_commands_run_in_order() {
        let temp = TempDir::new("execute-pre");
        let marker = temp.path().join("order.txt");
        let text = format!(
            "name = \"pre\"\narch = \"x86_64\"\nprofile = \"base\"\ntimeout_secs = 5\n\
             expect_exit = \"success\"\n\
             pre = [\"echo one >> {}\", \"echo two >> {}\"]\n",
            marker.display(),
            marker.display()
        );
        let case = Case::parse(&text).unwrap();
        let outcome = execute(&case, &runner("exit 33"));
        assert_eq!(
            outcome.report.result,
            ResultKind::Pass,
            "{:?}",
            outcome.detail
        );
        assert_eq!(std::fs::read_to_string(&marker).unwrap(), "one\ntwo\n");
    }

    #[test]
    fn a_failing_pre_command_is_an_error() {
        let temp = TempDir::new("execute-pre-fail");
        let marker = temp.path().join("ran.txt");
        let text = "name = \"pre-fail\"\narch = \"x86_64\"\nprofile = \"base\"\n\
                    timeout_secs = 5\nexpect_exit = \"success\"\n\
                    pre = [\"echo broken >&2; exit 7\"]\n";
        let case = Case::parse(text).unwrap();
        let outcome = execute(&case, &runner(&format!("touch {}", marker.display())));
        assert_eq!(outcome.report.result, ResultKind::Error);
        let detail = outcome.detail.expect("detail");
        assert!(detail.contains("pre[0]"), "{detail}");
        assert!(detail.contains("broken"), "{detail}");
        assert!(!marker.exists(), "the runner must not start");
    }

    #[test]
    fn a_skipped_case_does_not_run_pre_commands() {
        let temp = TempDir::new("execute-skip-pre");
        let marker = temp.path().join("ran.txt");
        let text = format!(
            "name = \"skip-pre\"\narch = \"x86_64\"\nprofile = \"base\"\ntimeout_secs = 5\n\
             expect_exit = \"success\"\nskip = \"later\"\npre = [\"touch {}\"]\n",
            marker.display()
        );
        let case = Case::parse(&text).unwrap();
        let outcome = execute(&case, &runner("exit 33"));
        assert_eq!(outcome.report.result, ResultKind::Skipped);
        assert!(!marker.exists(), "a skipped case must not run pre commands");
    }
}
