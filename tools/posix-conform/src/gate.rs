// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `posix-check`: compile/link/runtime probes and the published conformance report.
//!
//! The runner never turns a missing suite into a pass. Host-only mode deliberately skips the
//! QEMU target rows; target mode records a skipped target as a release blocker. Compile probes
//! use the ravenlibc include roots and the host `cc`; the target rows use the existing qtest
//! harness.

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, bail};
use serde::Serialize;

use crate::model::{InterfaceRow, Profile, Status, TraceFile};
use crate::overlay::{Claim, Overlay, OverlayOutcome};

/// Outer timeout for an overlay qtest claim. The case itself has a smaller boot timeout.
const QTEST_TIMEOUT_SECS: u64 = 900;
/// Timeout for a `test:` evidence script or binary.
const TEST_TIMEOUT_SECS: u64 = 300;

/// One probe verdict.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Verdict {
    /// The probe passed.
    Pass,
    /// The probe or requirement failed.
    Fail,
    /// The probe was not run; the reason is recorded.
    Skip,
}

impl Verdict {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Pass => "pass",
            Self::Fail => "fail",
            Self::Skip => "skip",
        }
    }
}

/// One published row.
#[derive(Clone, Debug, Serialize)]
pub struct ResultRow {
    /// Requirement or probe id.
    pub id: String,
    /// Area label.
    pub area: String,
    /// Verdict.
    pub result: Verdict,
    /// Command or evidence path used.
    pub command: String,
    /// Failure or skip reason.
    pub reason: String,
}

/// The complete gate report.
#[derive(Clone, Debug, Serialize)]
pub struct GateReport {
    /// Schema.
    pub schema: u32,
    /// Profile name.
    pub profile: String,
    /// Architecture.
    pub arch: String,
    /// Host-only flag.
    pub host_only: bool,
    /// UTC date string.
    pub date: String,
    /// Repository commit the gate ran against, or `unknown`.
    pub commit: String,
    /// Tool version.
    pub tool: String,
    /// Rows.
    pub rows: Vec<ResultRow>,
    /// Pass count.
    pub pass: usize,
    /// Fail count.
    pub fail: usize,
    /// Skip count.
    pub skip: usize,
    /// True when this is a release-blocking run.
    pub release_blocked: bool,
}

/// Gate options.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Options {
    /// Profile name; only `posix` exists.
    pub profile: String,
    /// Target architecture.
    pub arch: String,
    /// JSON summary output.
    pub json: bool,
    /// Skip QEMU target rows.
    pub host_only: bool,
    /// Explicit commit override; `None` reads `git rev-parse HEAD`.
    pub commit: Option<String>,
}

fn command_string(program: &str, args: &[String]) -> String {
    let mut text = program.to_string();
    for arg in args {
        text.push(' ');
        text.push_str(arg);
    }
    text
}

fn run_process(root: &Path, program: &str, args: &[String]) -> Result<(bool, String)> {
    let output = Command::new(program)
        .args(args)
        .current_dir(root)
        .output()
        .with_context(|| format!("cannot run {program}"))?;
    let mut text = String::new();
    text.push_str(&String::from_utf8_lossy(&output.stdout));
    text.push_str(&String::from_utf8_lossy(&output.stderr));
    Ok((output.status.success(), text))
}

fn timeout_args(program: &str, args: &[String], seconds: u64) -> Vec<String> {
    let mut full = vec![
        "--signal=KILL".to_string(),
        seconds.to_string(),
        program.to_string(),
    ];
    full.extend(args.iter().cloned());
    full
}

fn run_process_timeout(
    root: &Path,
    program: &str,
    args: &[String],
    seconds: u64,
) -> Result<(bool, Option<i32>, String)> {
    let full = timeout_args(program, args, seconds);
    let output = Command::new("timeout")
        .args(&full)
        .current_dir(root)
        .output()
        .with_context(|| format!("cannot run timeout for {program}"))?;
    let mut text = String::new();
    text.push_str(&String::from_utf8_lossy(&output.stdout));
    text.push_str(&String::from_utf8_lossy(&output.stderr));
    Ok((output.status.success(), output.status.code(), text))
}

fn last_line(text: &str) -> String {
    text.lines()
        .rev()
        .find(|line| !line.trim().is_empty())
        .unwrap_or("")
        .trim()
        .to_string()
}

/// Resolve the commit stamped into the report.
///
/// An explicit value wins; otherwise read `git rev-parse HEAD`. A missing git
/// checkout or an empty answer is recorded as `unknown`, never an invented SHA.
pub(crate) fn repository_commit(root: &Path, explicit: Option<&str>) -> String {
    if let Some(commit) = explicit.map(str::trim).filter(|value| !value.is_empty()) {
        return commit.to_string();
    }
    let args = vec!["rev-parse".to_string(), "HEAD".to_string()];
    match run_process(root, "git", &args) {
        Ok((true, text)) => {
            let commit = last_line(&text);
            if commit.is_empty() {
                "unknown".to_string()
            } else {
                commit
            }
        }
        _ => "unknown".to_string(),
    }
}

fn utc_date(root: &Path) -> String {
    let args = if let Ok(epoch) = std::env::var("SOURCE_DATE_EPOCH") {
        vec![
            "-u".to_string(),
            "-d".to_string(),
            format!("@{epoch}"),
            "+%Y-%m-%dT%H:%M:%SZ".to_string(),
        ]
    } else {
        vec!["-u".to_string(), "+%Y-%m-%dT%H:%M:%SZ".to_string()]
    };
    match run_process(root, "date", &args) {
        Ok((true, text)) => last_line(&text),
        _ => "unknown".to_string(),
    }
}

fn include_args(root: &Path, arch: &str) -> Vec<String> {
    vec![
        "-I".to_string(),
        root.join("libc/ravenlibc/include").display().to_string(),
        "-I".to_string(),
        root.join("libc/ravenlibc/arch")
            .join(arch)
            .join("include")
            .display()
            .to_string(),
    ]
}

fn compile_probe(
    root: &Path,
    arch: &str,
    source: &Path,
    output_dir: &Path,
) -> Result<(bool, String)> {
    let args = vec![
        "-fsyntax-only".to_string(),
        "-std=c17".to_string(),
        "-D_POSIX_C_SOURCE=202405L".to_string(),
        "-D_XOPEN_SOURCE=800".to_string(),
        "-Wno-unused-value".to_string(),
        source.display().to_string(),
    ];
    let mut full = include_args(root, arch);
    full.extend(args);
    let command = command_string("cc", &full);
    let result = run_process(root, "cc", &full)?;
    let _ = output_dir;
    Ok((result.0, command))
}

fn write_source(output_dir: &Path, name: &str, text: &str) -> Result<PathBuf> {
    std::fs::create_dir_all(output_dir)
        .with_context(|| format!("cannot create {}", output_dir.display()))?;
    let path = output_dir.join(name);
    std::fs::write(&path, text).with_context(|| format!("cannot write {}", path.display()))?;
    Ok(path)
}

fn header_compile(
    root: &Path,
    arch: &str,
    output_dir: &Path,
    index: usize,
    header: &str,
) -> Result<(bool, String)> {
    let source = write_source(
        output_dir,
        &format!("header_{index}.c"),
        &format!("#include <{header}>\nint main(void) {{ return 0; }}\n"),
    )?;
    compile_probe(root, arch, &source, output_dir)
}

fn interface_compile(
    root: &Path,
    arch: &str,
    output_dir: &Path,
    index: usize,
    row: &InterfaceRow,
) -> Result<(bool, String)> {
    let source = probe_source(row);
    let path = write_source(output_dir, &format!("interface_{index}.c"), &source)?;
    compile_probe(root, arch, &path, output_dir)
}

/// The C source used to prove one interface row's declaration.
///
/// A few seed rows carry a heuristic declaring header that is not the POSIX header
/// (`lockf` is declared in `<unistd.h>`, not `<sys/stat.h>`), and the macro rows cannot be
/// probed with `&name`. Both cases are handled here so the probe tests the declaration
/// rather than the seed heuristic.
#[must_use]
pub(crate) fn probe_source(row: &InterfaceRow) -> String {
    let header = match row.name.as_str() {
        "lockf" => "unistd.h",
        "statvfs" => "sys/statvfs.h",
        "strtof" => "stdlib.h",
        "wcsftime" => "wchar.h",
        "wcstoimax" | "wcstoumax" => "inttypes.h",
        _ => row.header.as_str(),
    };
    match row.name.as_str() {
        "offsetof" => format!(
            "#include <{header}>\nstruct probe_object {{ int member; }};\nstatic size_t probe_offset = offsetof(struct probe_object, member);\nint main(void) {{ return probe_offset == 0; }}\n"
        ),
        "va_start" | "va_end" => format!(
            "#include <{header}>\nstatic void probe_va(int count, ...) {{ va_list ap; va_start(ap, count); va_end(ap); }}\nint main(void) {{ probe_va(0); return 0; }}\n"
        ),
        "va_arg" => format!(
            "#include <{header}>\nstatic int probe_va(int count, ...) {{ va_list ap; va_start(ap, count); int value = va_arg(ap, int); va_end(ap); return value; }}\nint main(void) {{ return probe_va(1, 0); }}\n"
        ),
        "va_copy" => format!(
            "#include <{header}>\nstatic void probe_va(int count, ...) {{ va_list ap, copy; va_start(ap, count); va_copy(copy, ap); va_end(copy); va_end(ap); }}\nint main(void) {{ probe_va(0); return 0; }}\n"
        ),
        _ => format!(
            "#include <{header}>\nstatic void *const probe = (void *)&{};\nint main(void) {{ return 0; }}\n",
            row.name
        ),
    }
}

fn qtest_case(test: &str) -> Option<String> {
    let path = Path::new(test);
    if path.parent()?.file_name()? != "cases" {
        return None;
    }
    path.file_stem()?.to_str().map(str::to_string)
}

fn run_qtest(root: &Path, case: &str, arch: &str) -> Result<(Verdict, String, String)> {
    let args = vec![
        "xtask".to_string(),
        "qtest".to_string(),
        "--case".to_string(),
        case.to_string(),
        "--arch".to_string(),
        arch.to_string(),
    ];
    let command = command_string("cargo", &args);
    let (success, code, output) = run_process_timeout(root, "cargo", &args, QTEST_TIMEOUT_SECS)?;
    let json_path = root.join("build/qtest").join(format!("{case}.json"));
    if let Ok(text) = std::fs::read_to_string(&json_path)
        && let Ok(value) = serde_json::from_str::<serde_json::Value>(&text)
        && let Some(result) = value.get("result").and_then(serde_json::Value::as_str)
    {
        return Ok(match result {
            "pass" => (Verdict::Pass, command, String::new()),
            "fail" => (
                Verdict::Fail,
                command,
                format!("qtest: {}", last_line(&output)),
            ),
            _ => {
                let case_path = root.join("tests/qtest/cases").join(format!("{case}.toml"));
                let reason = std::fs::read_to_string(case_path)
                    .ok()
                    .and_then(|text| {
                        text.lines()
                            .find_map(|line| line.strip_prefix("skip = ").map(str::to_string))
                    })
                    .map(|value| value.trim_matches('"').to_string())
                    .unwrap_or_else(|| last_line(&output));
                (Verdict::Skip, command, format!("qtest skipped: {reason}"))
            }
        });
    }
    let verdict = if success {
        Verdict::Pass
    } else {
        Verdict::Fail
    };
    let reason = if code == Some(124) {
        format!("qtest timed out after {QTEST_TIMEOUT_SECS}s")
    } else {
        last_line(&output)
    };
    Ok((verdict, command, reason))
}

fn qtest_rows(root: &Path, arch: &str, host_only: bool) -> Vec<ResultRow> {
    let cases = [
        "libc-hello",
        "libc-stdio",
        "libc-time",
        "libc-threads",
        "libc-signals",
        "libc-dynamic",
        "libc-spawn",
    ];
    let mut rows = Vec::new();
    for case in cases {
        if host_only {
            rows.push(ResultRow {
                id: format!("target.{case}"),
                area: "target".to_string(),
                result: Verdict::Skip,
                command: format!("cargo xtask qtest --case {case} --arch {arch}"),
                reason: "host-only run; target not booted".to_string(),
            });
            continue;
        }
        let (result, command, reason) = run_qtest(root, case, arch).unwrap_or_else(|error| {
            (
                Verdict::Fail,
                format!("cargo xtask qtest --case {case} --arch {arch}"),
                format!("{error:#}"),
            )
        });
        rows.push(ResultRow {
            id: format!("target.{case}"),
            area: "target".to_string(),
            result,
            command,
            reason,
        });
    }
    rows
}

fn external_rows() -> Vec<ResultRow> {
    let suites = [
        (
            "external.posix-test-suite",
            "POSIX_TEST_SUITE",
            "external Open POSIX Test Suite (GPL); run it outside the repository",
        ),
        (
            "external.pjdfstest",
            "PJDFSTEST",
            "external pjdfstest (BSD); run it outside the repository",
        ),
        (
            "external.atf",
            "ATF",
            "donor ATF results are recorded in tests/libc/CONFORMANCE.md; run cargo test -p raven-atf-run separately",
        ),
    ];
    suites
        .into_iter()
        .map(|(id, variable, reason)| {
            let configured = std::env::var_os(variable).is_some();
            ResultRow {
                id: id.to_string(),
                area: "external".to_string(),
                result: Verdict::Skip,
                command: format!("{variable}=<path>"),
                reason: if configured {
                    format!("configured externally; {reason}")
                } else {
                    format!("{variable} not set; {reason}")
                },
            }
        })
        .collect()
}

fn trace_tests(trace: &TraceFile) -> BTreeMap<String, String> {
    trace
        .trace
        .iter()
        .map(|row| (row.id.clone(), row.test.clone()))
        .collect()
}

fn add_row(
    rows: &mut Vec<ResultRow>,
    id: &str,
    area: &str,
    verdict: Verdict,
    command: &str,
    reason: &str,
) {
    rows.push(ResultRow {
        id: id.to_string(),
        area: area.to_string(),
        result: verdict,
        command: command.to_string(),
        reason: reason.to_string(),
    });
}

fn run_qtest_cached(
    root: &Path,
    case: &str,
    arch: &str,
    cache: &mut BTreeMap<String, (Verdict, String, String)>,
) -> Result<(Verdict, String, String)> {
    if let Some(hit) = cache.get(case) {
        return Ok(hit.clone());
    }
    let outcome = run_qtest(root, case, arch)?;
    cache.insert(case.to_string(), outcome.clone());
    Ok(outcome)
}

#[allow(clippy::too_many_arguments)] // one row needs the profile, trace, cache and output sink
fn gate_behaviour(
    root: &Path,
    arch: &str,
    area: &str,
    meta: &crate::model::Meta,
    tests: &BTreeMap<String, String>,
    host_only: bool,
    cache: &mut BTreeMap<String, (Verdict, String, String)>,
    rows: &mut Vec<ResultRow>,
) {
    let test = tests
        .get(&meta.id)
        .cloned()
        .unwrap_or_else(|| "none".to_string());
    if meta.status == Status::Present && test != "none" {
        if host_only {
            add_row(
                rows,
                &meta.id,
                area,
                Verdict::Skip,
                &test,
                "host-only run; target test not executed",
            );
            return;
        }
        let Some(case) = qtest_case(&test) else {
            add_row(
                rows,
                &meta.id,
                area,
                Verdict::Fail,
                &test,
                "test is not a qtest case",
            );
            return;
        };
        let (result, command, reason) = run_qtest_cached(root, &case, arch, cache)
            .unwrap_or_else(|error| (Verdict::Fail, test.clone(), format!("{error:#}")));
        add_row(rows, &meta.id, area, result, &command, &reason);
        return;
    }
    add_row(
        rows,
        &meta.id,
        area,
        Verdict::Fail,
        meta.evidence.as_str(),
        &format!(
            "profile status {}; no executable POSIX behaviour test",
            meta.status.as_str()
        ),
    );
}

fn gate_rows(
    root: &Path,
    arch: &str,
    profile: &Profile,
    trace: &TraceFile,
    host_only: bool,
    qtest_cache: &mut BTreeMap<String, (Verdict, String, String)>,
) -> Result<Vec<ResultRow>> {
    let mut rows = Vec::new();
    let output_dir = root.join("build/posix-gate").join(arch);
    std::fs::create_dir_all(&output_dir)?;
    let tests = trace_tests(trace);
    let mut cc_available = true;
    if Command::new("cc").arg("--version").output().is_err() {
        cc_available = false;
    }
    let compile_supported = arch == "x86_64";

    for (index, header) in profile.header.iter().enumerate() {
        if header.meta.status != Status::Present {
            add_row(
                &mut rows,
                &header.meta.id,
                "header",
                Verdict::Fail,
                "profile",
                &format!("profile status {}", header.meta.status.as_str()),
            );
            continue;
        }
        if !compile_supported {
            add_row(
                &mut rows,
                &header.meta.id,
                "header",
                Verdict::Skip,
                "host-only compile",
                "host-only compile probes require --arch x86_64",
            );
            continue;
        }
        if !cc_available {
            add_row(
                &mut rows,
                &header.meta.id,
                "header",
                Verdict::Skip,
                "cc --version",
                "cc not found",
            );
            continue;
        }
        let (ok, command) = header_compile(root, arch, &output_dir, index, &header.name)?;
        add_row(
            &mut rows,
            &header.meta.id,
            "header",
            if ok { Verdict::Pass } else { Verdict::Fail },
            &command,
            if ok { "" } else { "header did not compile" },
        );
    }

    for (index, interface) in profile.interface.iter().enumerate() {
        if interface.meta.status != Status::Present {
            add_row(
                &mut rows,
                &interface.meta.id,
                "interface",
                Verdict::Fail,
                "profile",
                &format!("profile status {}", interface.meta.status.as_str()),
            );
            continue;
        }
        if !compile_supported {
            add_row(
                &mut rows,
                &interface.meta.id,
                "interface",
                Verdict::Skip,
                "host-only compile",
                "host-only compile probes require --arch x86_64",
            );
            continue;
        }
        if !cc_available {
            add_row(
                &mut rows,
                &interface.meta.id,
                "interface",
                Verdict::Skip,
                "cc --version",
                "cc not found",
            );
            continue;
        }
        let (ok, command) = interface_compile(root, arch, &output_dir, index, interface)?;
        add_row(
            &mut rows,
            &interface.meta.id,
            "interface",
            if ok { Verdict::Pass } else { Verdict::Fail },
            &command,
            if ok {
                ""
            } else {
                "declaration did not compile"
            },
        );
    }

    // Utilities and behaviour rows use the profile status plus the trace test, if any.
    // One qtest case may back several rows; run it once per gate invocation.
    for utility in &profile.utility {
        let test = tests
            .get(&utility.meta.id)
            .cloned()
            .unwrap_or_else(|| "none".to_string());
        match (utility.meta.status, test.as_str()) {
            (Status::Present, test) if test != "none" => {
                if host_only {
                    add_row(
                        &mut rows,
                        &utility.meta.id,
                        "utility",
                        Verdict::Skip,
                        test,
                        "host-only run; target test not executed",
                    );
                } else if let Some(case) = qtest_case(test) {
                    let (result, command, reason) =
                        run_qtest_cached(root, &case, arch, qtest_cache).unwrap_or_else(|error| {
                            (Verdict::Fail, test.to_string(), format!("{error:#}"))
                        });
                    add_row(
                        &mut rows,
                        &utility.meta.id,
                        "utility",
                        result,
                        &command,
                        &reason,
                    );
                } else {
                    add_row(
                        &mut rows,
                        &utility.meta.id,
                        "utility",
                        Verdict::Fail,
                        test,
                        "test is not a qtest case",
                    );
                }
            }
            (status, _) => add_row(
                &mut rows,
                &utility.meta.id,
                "utility",
                Verdict::Fail,
                utility.meta.evidence.as_str(),
                &format!("profile status {}; no POSIX utility test", status.as_str()),
            ),
        }
    }

    for entry in &profile.shell_feature {
        gate_behaviour(
            root,
            arch,
            "shell",
            &entry.meta,
            &tests,
            host_only,
            qtest_cache,
            &mut rows,
        );
    }
    for entry in &profile.filesystem {
        gate_behaviour(
            root,
            arch,
            "filesystem",
            &entry.meta,
            &tests,
            host_only,
            qtest_cache,
            &mut rows,
        );
    }
    for entry in &profile.process {
        gate_behaviour(
            root,
            arch,
            "process",
            &entry.meta,
            &tests,
            host_only,
            qtest_cache,
            &mut rows,
        );
    }
    for entry in &profile.tty {
        gate_behaviour(
            root,
            arch,
            "tty",
            &entry.meta,
            &tests,
            host_only,
            qtest_cache,
            &mut rows,
        );
    }

    let libc_archive = root.join("build").join(arch).join("libc.a");
    if libc_archive.is_file() {
        let args = vec![
            "-g".to_string(),
            "--defined-only".to_string(),
            libc_archive.display().to_string(),
        ];
        let (ok, output) = run_process(root, "nm", &args)?;
        if ok {
            let present: Vec<&str> = profile
                .interface
                .iter()
                .filter(|row| row.meta.status == Status::Present && row.kind == "func")
                .map(|row| row.name.as_str())
                .collect();
            let missing = present
                .iter()
                .filter(|name| !output.contains(*name))
                .count();
            add_row(
                &mut rows,
                "link.libc",
                "link",
                if missing == 0 {
                    Verdict::Pass
                } else {
                    Verdict::Fail
                },
                &command_string("nm", &args),
                &format!("{missing} required symbols absent from libc.a"),
            );
        } else {
            add_row(
                &mut rows,
                "link.libc",
                "link",
                Verdict::Fail,
                &command_string("nm", &args),
                "nm failed on libc.a",
            );
        }
    } else {
        add_row(
            &mut rows,
            "link.libc",
            "link",
            Verdict::Skip,
            "nm build/<arch>/libc.a",
            "libc.a is not built; run cargo xtask libc-link --arch <arch>",
        );
    }

    rows.extend(external_rows());
    rows.extend(qtest_rows(root, arch, host_only));
    Ok(rows)
}

fn run_test_evidence(root: &Path, relative: &Path) -> Result<(bool, String)> {
    use std::os::unix::fs::PermissionsExt;

    let path = root.join(relative);
    let metadata = std::fs::metadata(&path)
        .with_context(|| format!("cannot stat test evidence {}", path.display()))?;
    let bytes = std::fs::read(&path)
        .with_context(|| format!("cannot read test evidence {}", path.display()))?;
    let first = bytes.split(|byte| *byte == b'\n').next().unwrap_or(&[]);
    let first = String::from_utf8_lossy(first);
    let (program, mut args) = if let Some(rest) = first.trim().strip_prefix("#!") {
        let mut parts = rest.split_whitespace();
        let program = parts
            .next()
            .ok_or_else(|| anyhow::anyhow!("test `{}` has an empty shebang", relative.display()))?;
        (
            program.to_string(),
            parts.map(str::to_string).collect::<Vec<_>>(),
        )
    } else {
        if metadata.permissions().mode() & 0o111 == 0 {
            bail!(
                "test `{}` has no shebang and is not executable",
                relative.display()
            );
        }
        (path.display().to_string(), Vec::new())
    };
    args.push(path.display().to_string());
    let (success, code, output) = run_process_timeout(root, &program, &args, TEST_TIMEOUT_SECS)?;
    if code == Some(124) {
        return Ok((
            false,
            format!("test evidence timed out after {TEST_TIMEOUT_SECS}s"),
        ));
    }
    if success {
        return Ok((true, String::new()));
    }
    let last = last_line(&output);
    Ok((
        false,
        if last.is_empty() {
            format!("test evidence `{}` failed", relative.display())
        } else {
            format!("test evidence `{}` failed: {last}", relative.display())
        },
    ))
}

fn set_overlay_row(
    rows: &mut Vec<ResultRow>,
    id: &str,
    area: &str,
    result: Verdict,
    command: String,
    reason: String,
) {
    if let Some(row) = rows.iter_mut().find(|row| row.id == id) {
        row.result = result;
        row.command = command;
        row.reason = reason;
    } else {
        rows.push(ResultRow {
            id: id.to_string(),
            area: area.to_string(),
            result,
            command,
            reason,
        });
    }
}

/// Execute and apply the owner overlay to the published rows.
///
/// A supported `present` claim must run its qtest or test evidence; a failing or timed-out
/// evidence target becomes a failure. A `waived` claim passes only with a written waiver, and the
/// waiver stays visible in the row reason. An unsupported claim is reported as a failure.
#[allow(clippy::too_many_arguments)] // rows, profile, overlay and the shared qtest cache
fn apply_overlay(
    root: &Path,
    arch: &str,
    rows: &mut Vec<ResultRow>,
    profile: &Profile,
    outcome: &OverlayOutcome,
    host_only: bool,
    qtest_cache: &mut BTreeMap<String, (Verdict, String, String)>,
) {
    let areas: BTreeMap<&str, &str> = profile
        .metas()
        .into_iter()
        .map(|(area, meta)| (meta.id.as_str(), area))
        .collect();
    // Many requirements cite the same evidence script; run each path once per gate.
    let mut test_cache: BTreeMap<String, (bool, String)> = BTreeMap::new();
    for claim in &outcome.claims {
        let id = claim.id();
        let area = areas.get(id).copied().unwrap_or("overlay");
        let (result, command, reason) = match claim {
            Claim::Qtest { case, .. } => {
                if host_only {
                    (
                        Verdict::Skip,
                        claim.command(),
                        "host-only run; qtest evidence not executed".to_string(),
                    )
                } else {
                    match run_qtest_cached(root, case, arch, qtest_cache) {
                        Ok((Verdict::Pass, _, _)) => {
                            (Verdict::Pass, claim.command(), String::new())
                        }
                        Ok((Verdict::Fail, _, reason)) => (
                            Verdict::Fail,
                            claim.command(),
                            if reason.is_empty() {
                                format!("qtest `{case}` failed")
                            } else {
                                reason
                            },
                        ),
                        Ok((Verdict::Skip, _, reason)) => (
                            Verdict::Fail,
                            claim.command(),
                            format!("qtest `{case}` was skipped: {reason}"),
                        ),
                        Err(error) => (Verdict::Fail, claim.command(), format!("{error:#}")),
                    }
                }
            }
            Claim::Test { path, .. } => {
                let key = path.display().to_string();
                let (ok, reason) = match test_cache.get(&key) {
                    Some(hit) => hit.clone(),
                    None => {
                        let outcome = run_test_evidence(root, path)
                            .unwrap_or_else(|error| (false, format!("{error:#}")));
                        test_cache.insert(key, outcome.clone());
                        outcome
                    }
                };
                if ok {
                    (Verdict::Pass, claim.command(), String::new())
                } else {
                    (Verdict::Fail, claim.command(), reason)
                }
            }
            Claim::Waived { waiver, .. } => (
                Verdict::Pass,
                claim.command(),
                format!("owner waiver: {waiver}"),
            ),
        };
        set_overlay_row(rows, id, area, result, command, reason);
    }
    for claim in &outcome.unsupported {
        let area = areas.get(claim.id.as_str()).copied().unwrap_or("overlay");
        set_overlay_row(
            rows,
            &claim.id,
            area,
            Verdict::Fail,
            "mgmt posix/evidence".to_string(),
            claim.reason.clone(),
        );
    }
}

/// Run the gate and return the report plus target rows.
pub fn run(
    root: &Path,
    profile: &Profile,
    trace: &TraceFile,
    options: &Options,
) -> Result<GateReport> {
    if options.profile != "posix" {
        bail!("unknown profile `{}`; only `posix` exists", options.profile);
    }
    if !matches!(options.arch.as_str(), "x86_64" | "aarch64" | "riscv64") {
        bail!("unknown arch `{}`", options.arch);
    }
    let overlay = Overlay::load(root)?;
    let mut merged = profile.clone();
    let outcome = overlay.apply(root, &mut merged);
    let mut qtest_cache: BTreeMap<String, (Verdict, String, String)> = BTreeMap::new();
    let mut rows = gate_rows(
        root,
        &options.arch,
        &merged,
        trace,
        options.host_only,
        &mut qtest_cache,
    )?;
    apply_overlay(
        root,
        &options.arch,
        &mut rows,
        &merged,
        &outcome,
        options.host_only,
        &mut qtest_cache,
    );
    rows.sort_by(|left, right| {
        (left.area.as_str(), left.id.as_str()).cmp(&(right.area.as_str(), right.id.as_str()))
    });
    let pass = rows
        .iter()
        .filter(|row| row.result == Verdict::Pass)
        .count();
    let fail = rows
        .iter()
        .filter(|row| row.result == Verdict::Fail)
        .count();
    let skip = rows
        .iter()
        .filter(|row| row.result == Verdict::Skip)
        .count();
    let release_blocked = fail > 0
        || (!options.host_only
            && rows
                .iter()
                .any(|row| row.area == "target" && row.result == Verdict::Skip));
    Ok(GateReport {
        schema: 1,
        profile: options.profile.clone(),
        arch: options.arch.clone(),
        host_only: options.host_only,
        date: utc_date(root),
        commit: repository_commit(root, options.commit.as_deref()),
        tool: format!("posix-conform {}", env!("CARGO_PKG_VERSION")),
        rows,
        pass,
        fail,
        skip,
        release_blocked,
    })
}

/// Render the published `CONFORMANCE.md`.
pub fn render(report: &GateReport) -> String {
    let mut output = String::new();
    let _ = writeln!(output, "<!-- SPDX-License-Identifier: BSD-3-Clause -->");
    let _ = writeln!(
        output,
        "<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->"
    );
    let _ = writeln!(
        output,
        "<!-- GENERATED by cargo xtask posix-check; do not edit. -->"
    );
    let _ = writeln!(output, "\n# POSIX.1-2024 self-certification\n");
    let _ = writeln!(
        output,
        "This is the RavenBSD self-certification gate, not Open Group certification. \
         A green `make check` with a red `posix-check` is not a release."
    );
    let _ = writeln!(output, "\n- Date: {}", report.date);
    let _ = writeln!(output, "- Commit: `{}`", report.commit);
    let _ = writeln!(output, "- Profile: `{}`", report.profile);
    let _ = writeln!(output, "- Target: `{}`", report.arch);
    let _ = writeln!(
        output,
        "- Mode: {}",
        if report.host_only {
            "host-only"
        } else {
            "target QEMU"
        }
    );
    let _ = writeln!(output, "- Tool: `{}`", report.tool);
    let _ = writeln!(
        output,
        "- Result: **{}**",
        if report.release_blocked {
            "FAIL (release blocked)"
        } else {
            "PASS"
        }
    );
    let _ = writeln!(
        output,
        "- Counts: {} pass, {} fail, {} skip",
        report.pass, report.fail, report.skip
    );

    let mut areas: BTreeMap<&str, [usize; 3]> = BTreeMap::new();
    for row in &report.rows {
        let counts = areas.entry(row.area.as_str()).or_insert([0; 3]);
        match row.result {
            Verdict::Pass => counts[0] += 1,
            Verdict::Fail => counts[1] += 1,
            Verdict::Skip => counts[2] += 1,
        }
    }
    let _ = writeln!(output, "\n## Summary\n");
    let _ = writeln!(output, "| area | pass | fail | skip |");
    let _ = writeln!(output, "|---|---:|---:|---:|");
    for (area, counts) in &areas {
        let _ = writeln!(
            output,
            "| {area} | {} | {} | {} |",
            counts[0], counts[1], counts[2]
        );
    }
    let _ = writeln!(
        output,
        "| **all** | **{}** | **{}** | **{}** |",
        report.pass, report.fail, report.skip
    );

    let _ = writeln!(output, "\n## Rows\n");
    let _ = writeln!(output, "| id | area | result | command | reason |");
    let _ = writeln!(output, "|---|---|---|---|---|");
    for row in &report.rows {
        let _ = writeln!(
            output,
            "| `{}` | {} | {} | `{}` | {} |",
            row.id,
            row.area,
            row.result.as_str(),
            row.command.replace('|', "\\|"),
            row.reason.replace('|', "\\|")
        );
    }
    output
}

/// JSON summary.
pub fn json(report: &GateReport) -> Result<String> {
    Ok(serde_json::to_string(report)?)
}
