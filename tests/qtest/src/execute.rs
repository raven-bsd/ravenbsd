// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Case execution: image preparation, process launch, matching and result classification.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

use crate::case::{Case, ExpectExit};
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
    /// Do not run the case's `pre` commands; the image/fixture must already exist.
    pub no_pre: bool,
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
            no_pre: false,
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

/// A prepared QEMU command plus the per-run artifacts that must be released afterwards.
struct Prepared {
    program: String,
    args: Vec<String>,
    cleanup: Vec<PathBuf>,
}

/// Remove per-run artifacts that are safe to discard once QEMU is reaped.
///
/// The case image is deliberately kept: `--no-build` runs reuse it, and QEMU no longer holds it
/// after the process group is killed. Only the OVMF/AAVMF vars copy belongs to this run.
fn remove_run_artifacts(paths: &[PathBuf]) {
    for path in paths {
        let _ = fs::remove_file(path);
    }
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
            false,
        );
    }

    if !options.no_pre
        && let Err(error) = run_pre_commands(case, &arch)
    {
        let (reason, kind) = match error {
            PrepareError::Skipped(reason) => (reason, ResultKind::Skipped),
            PrepareError::Error(reason) => (reason, ResultKind::Error),
        };
        return finish(report, Vec::new(), Some(reason), kind, started, false);
    }

    let mut cleanup = Vec::new();
    let (program, args) = match &options.runner {
        Some(runner) if runner.is_empty() => {
            return finish(
                report,
                Vec::new(),
                Some("--runner needs a command".to_string()),
                ResultKind::Error,
                started,
                options.no_pre,
            );
        }
        Some(runner) => (runner[0].clone(), runner[1..].to_vec()),
        None => match prepare_external(case, &arch, options) {
            Ok(prepared) => {
                cleanup = prepared.cleanup;
                (prepared.program, prepared.args)
            }
            Err(PrepareError::Skipped(reason)) => {
                return finish(
                    report,
                    Vec::new(),
                    Some(reason),
                    ResultKind::Skipped,
                    started,
                    options.no_pre,
                );
            }
            Err(PrepareError::Error(reason)) => {
                return finish(
                    report,
                    Vec::new(),
                    Some(reason),
                    ResultKind::Error,
                    started,
                    options.no_pre,
                );
            }
        },
    };

    let timeout = Duration::from_secs(case.spec.timeout_secs);
    let spec = RunSpec {
        program,
        args,
        timeout,
        keep_ansi: case.spec.keep_ansi,
    };
    // Only real QEMU runs need the incremental evidence log; fake `--runner` tests stay
    // side-effect free.
    let log_path = options
        .runner
        .is_none()
        .then(|| options.out_dir.join(format!("{}.log", case.spec.name)));
    let mut dialogue = Dialogue::from_case(case);
    let run = match dialogue.as_mut() {
        Some(active) => runner::run_watched(&spec, Some(active), case, log_path.as_deref()),
        None => runner::run_watched(&spec, None, case, log_path.as_deref()),
    };
    remove_run_artifacts(&cleanup);
    let run = match run {
        Ok(run) => run,
        Err(error) => {
            return finish(
                report,
                Vec::new(),
                Some(format!("could not start case: {error:#}")),
                ResultKind::Error,
                started,
                options.no_pre,
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

    // QEMU occasionally stays alive after the guest's final marker. The harness reaps it after a
    // short grace; the complete expectation set then stands in for the exit status, exactly as
    // the ticket requires. Failure/timeout expectations keep their stricter meaning.
    let forced_completion = run.completed && run.status.is_none();
    let exit_ok = if run.timed_out {
        exitmap::timeout_matches(case.spec.expect_exit)
    } else if forced_completion {
        matches!(case.spec.expect_exit, ExpectExit::Success | ExpectExit::Any)
    } else {
        exitmap::expected_matches(case.spec.expect_exit, exit)
    };
    let passed = exit_ok && report.unmatched.is_empty() && report.forbidden_hits.is_empty();
    let step_failure = run.step_failure;
    let result = if step_failure.is_some() || run.forbidden_failure {
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
    finish(report, run.lines, detail, result, started, options.no_pre)
}

fn prepare_external(
    case: &Case,
    arch: &str,
    options: &ExecuteOptions,
) -> Result<Prepared, PrepareError> {
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
    let vars_path = vars.clone();
    let firmware = qemu::Firmware {
        code: firmware.code,
        vars,
    };
    let command = qemu::qemu_command(
        arch,
        &image,
        &firmware,
        &case.spec.qemu_extra,
        case.spec.disk_if.as_deref(),
        case.spec.disk_format.as_deref(),
    )
    .map_err(PrepareError::Error)?;
    Ok(Prepared {
        program: command.program,
        args: command.args,
        cleanup: vec![vars_path],
    })
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
    if let Some(image) = &case.spec.image {
        let path = PathBuf::from(image);
        if !path.is_file() {
            return Err(PrepareError::Error(format!(
                "case image not found: {} (set by `image`; the pre step must build it)",
                path.display()
            )));
        }
        return Ok(path);
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
/// and before the image build. Exit 77 is the preflight's waiting-for-window signal and maps to
/// `skipped`; any other non-zero exit is an error. Exit 77 is deliberately distinct so a
/// latency-row preflight cannot silently turn into a pass or a product failure.
fn run_pre_commands(case: &Case, arch: &str) -> Result<(), PrepareError> {
    for (index, command) in case.spec.pre.iter().enumerate() {
        let output = Command::new("sh")
            .arg("-c")
            .arg(command)
            .env("RAVEN_QT_ARCH", arch)
            .output()
            .map_err(|error| {
                PrepareError::Error(format!("pre[{index}] `{command}` could not start: {error}"))
            })?;
        if output.status.success() {
            continue;
        }
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stdout = String::from_utf8_lossy(&output.stdout);
        let message = last_line(&stderr)
            .or_else(|| last_line(&stdout))
            .unwrap_or("no output");
        let reason = format!(
            "pre[{index}] `{command}` failed with {}: {message}",
            output.status
        );
        return Err(if output.status.code() == Some(77) {
            PrepareError::Skipped(reason)
        } else {
            PrepareError::Error(reason)
        });
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
    pre_skipped: bool,
) -> CaseOutcome {
    let detail = if pre_skipped {
        Some(match detail {
            Some(existing) => format!("{existing}; pre skipped: --no-pre"),
            None => "pre skipped: --no-pre".to_string(),
        })
    } else {
        detail
    };
    report.result = result;
    report.duration_ms = u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX);
    CaseOutcome {
        report,
        lines,
        detail,
    }
}

#[cfg(test)]
#[path = "execute_ut.rs"]
mod tests;
