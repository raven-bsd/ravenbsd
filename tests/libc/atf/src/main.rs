// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `raven-atf-run` command-line entry point.

#![forbid(unsafe_code)]

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result, bail};
use raven_atf_run::check::{self, CheckReport};
use raven_atf_run::gate::{self, GateConfig};
use raven_atf_run::report;
use raven_atf_run::runner::{self, RunOptions};
use raven_atf_run::suite::{self, RunConfig};
use raven_atf_run::table::{self, ConformanceTable, RawRow, TableRow};
use raven_atf_run::{
    Case, EXIT_CHECK_FAILED, EXIT_GATE_NOT_MET, EXIT_NO_TOOL, EXIT_OK, EXIT_USAGE, Mode, Outcome,
    RunBackend, Verdict,
};

const USAGE: &str = "\
usage:
  raven-atf-run list      <program> [--srcdir DIR]
  raven-atf-run run       <program> [-t CASE] [--srcdir DIR] [--timeout S] [--var K=V]...
                          [--exclude-file F] [--report human|tap|json|junit]
  raven-atf-run exit-status <program> [--args A...] [--expect-exit N] [--timeout S]
  raven-atf-run gen-suite (--all | --suite NAME) [--resources DIR]
                          [--vendor tests/libc-oracle] [--out DIR]
  raven-atf-run suite     --suite NAME --mode oracle|target [--sysroot DIR]
                          [--run-backend none|host-linux|qemu] [--cc CMD]
                          [--out build/libc-conformance] [--filter GLOB]
                          [--jobs N] [--max-timeout S] [--keep] [--var K=V]...
                          [--target host-oracle|host-target|qemu-<arch>]
  raven-atf-run table     [--write] [--runs DIR] [--baseline FILE] [--md FILE]
                          [--accept-regressions REASON]
  raven-atf-run check     [--table FILE] [--run FILE|DIR]
  raven-atf-run gate      --g1 [--table FILE] [--config FILE]";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match run(&args) {
        Ok(code) => ExitCode::from(code),
        Err(error) => {
            eprintln!("raven-atf-run: {error:#}");
            ExitCode::from(exit_code(&error))
        }
    }
}

fn exit_code(error: &anyhow::Error) -> u8 {
    let text = format!("{error:#}");
    if text.contains("required host tool") {
        EXIT_NO_TOOL
    } else {
        EXIT_USAGE
    }
}

fn run(args: &[String]) -> Result<u8> {
    let Some(command) = args.first() else {
        bail!("{USAGE}");
    };
    let rest = &args[1..];
    match command.as_str() {
        "list" => command_list(rest),
        "run" => command_run(rest),
        "exit-status" => command_exit_status(rest),
        "gen-suite" => command_gen_suite(rest),
        "suite" => command_suite(rest),
        "table" => command_table(rest),
        "check" => command_check(rest),
        "gate" => command_gate(rest),
        "-h" | "--help" | "help" => {
            println!("{USAGE}");
            Ok(EXIT_OK)
        }
        other => bail!("unknown subcommand `{other}`\n{USAGE}"),
    }
}

fn repo_root() -> PathBuf {
    if let Ok(root) = std::env::var("RL_REPO_ROOT") {
        return PathBuf::from(root);
    }
    let Ok(current) = std::env::current_dir() else {
        return PathBuf::from(".");
    };
    let mut candidate = current.as_path();
    loop {
        if candidate.join("tests/libc/atf").is_dir() && candidate.join("Cargo.toml").is_file() {
            return candidate.to_path_buf();
        }
        match candidate.parent() {
            Some(parent) => candidate = parent,
            None => return current,
        }
    }
}

fn resolve(root: &Path, path: &str) -> PathBuf {
    let path = PathBuf::from(path);
    if path.is_absolute() {
        path
    } else {
        root.join(path)
    }
}

struct ValueParser<'a> {
    args: &'a [String],
    index: usize,
}

impl<'a> ValueParser<'a> {
    fn new(args: &'a [String]) -> Self {
        Self { args, index: 0 }
    }

    fn next(&mut self) -> Option<&'a str> {
        let value = self.args.get(self.index)?;
        self.index += 1;
        Some(value)
    }

    fn value(&mut self, flag: &str) -> Result<&'a str> {
        self.next()
            .ok_or_else(|| anyhow::anyhow!("{flag} needs a value\n{USAGE}"))
    }
}

fn command_list(args: &[String]) -> Result<u8> {
    let mut parser = ValueParser::new(args);
    let Some(program) = parser.next() else {
        bail!("list needs a program\n{USAGE}");
    };
    let mut srcdir = None;
    while let Some(flag) = parser.next() {
        match flag {
            "--srcdir" => srcdir = Some(PathBuf::from(parser.value("--srcdir")?)),
            other => bail!("unknown list flag `{other}`\n{USAGE}"),
        }
    }
    let program = resolve(&repo_root(), program);
    let source = srcdir.or_else(|| program.parent().map(Path::to_path_buf));
    let cases = runner::list(&program, source.as_deref(), 60)?;
    for case in cases {
        println!("{}", case.ident);
    }
    Ok(EXIT_OK)
}

fn command_run(args: &[String]) -> Result<u8> {
    let mut parser = ValueParser::new(args);
    let Some(program) = parser.next() else {
        bail!("run needs a program\n{USAGE}");
    };
    let program = resolve(&repo_root(), program);
    let mut only = None;
    let mut srcdir = None;
    let mut timeout = 300u64;
    let mut vars = BTreeMap::new();
    let mut exclude_file = None;
    let mut format = report::Format::Human;
    while let Some(flag) = parser.next() {
        match flag {
            "-t" => only = Some(parser.value("-t")?.to_string()),
            "--srcdir" => srcdir = Some(PathBuf::from(parser.value("--srcdir")?)),
            "--timeout" => timeout = parse_u64(parser.value("--timeout")?, "--timeout")?,
            "--var" => {
                let (key, value) = split_var(parser.value("--var")?)?;
                vars.insert(key, value);
            }
            "--exclude-file" => exclude_file = Some(PathBuf::from(parser.value("--exclude-file")?)),
            "--report" => {
                let value = parser.value("--report")?;
                format = report::Format::parse(value)
                    .ok_or_else(|| anyhow::anyhow!("unknown report `{value}`\n{USAGE}"))?;
            }
            other => bail!("unknown run flag `{other}`\n{USAGE}"),
        }
    }
    let source = srcdir.or_else(|| program.parent().map(Path::to_path_buf));
    let mut cases = runner::list(&program, source.as_deref(), timeout)?;
    if let Some(only) = only {
        cases.retain(|case| case.ident == only);
        if cases.is_empty() {
            bail!("case `{only}` is not in the program's listing");
        }
    }
    if let Some(file) = exclude_file {
        let text =
            fs::read_to_string(&file).with_context(|| format!("cannot read {}", file.display()))?;
        let excluded: Vec<&str> = text
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty() && !line.starts_with('#'))
            .collect();
        cases.retain(|case| !excluded.contains(&case.ident.as_str()));
    }
    let work = std::env::temp_dir().join(format!("raven-atf-run-cli-{}", std::process::id()));
    fs::create_dir_all(&work).with_context(|| format!("cannot create {}", work.display()))?;
    let options = RunOptions {
        timeout_secs: timeout,
        vars,
        keep: false,
    };
    let label = program
        .file_name()
        .map_or_else(String::new, |name| name.to_string_lossy().into_owned());
    let mut outcomes = Vec::new();
    for case in &cases {
        let observation = runner::run_case(&program, case, source.as_deref(), &work, &options)?;
        outcomes.push(Outcome::new(
            Case::new(label.clone(), case.ident.clone(), case.ident.clone()),
            observation.verdict,
            observation.reason,
            "-",
        ));
    }
    let _ = fs::remove_dir_all(&work);
    print!("{}", report::render(&outcomes, format));
    Ok(
        if outcomes
            .iter()
            .any(|outcome| outcome.verdict == Verdict::Fail)
        {
            EXIT_CHECK_FAILED
        } else {
            EXIT_OK
        },
    )
}

fn command_exit_status(args: &[String]) -> Result<u8> {
    let mut parser = ValueParser::new(args);
    let Some(program) = parser.next() else {
        bail!("exit-status needs a program\n{USAGE}");
    };
    let program = resolve(&repo_root(), program);
    let mut program_args = Vec::new();
    let mut expect_exit = 0i32;
    let mut timeout = 300u64;
    while let Some(flag) = parser.next() {
        match flag {
            "--args" => {
                while let Some(value) = parser.next() {
                    if value.starts_with("--") {
                        bail!("--args must be the last flag (got `{value}`)");
                    }
                    program_args.push(value.to_string());
                }
            }
            "--expect-exit" => {
                expect_exit = parser
                    .value("--expect-exit")?
                    .parse()
                    .map_err(|_| anyhow::anyhow!("--expect-exit needs an integer"))?;
            }
            "--timeout" => timeout = parse_u64(parser.value("--timeout")?, "--timeout")?,
            other => bail!("unknown exit-status flag `{other}`\n{USAGE}"),
        }
    }
    let work = std::env::temp_dir().join(format!("raven-atf-run-exit-{}", std::process::id()));
    fs::create_dir_all(&work).with_context(|| format!("cannot create {}", work.display()))?;
    let options = RunOptions {
        timeout_secs: timeout,
        vars: BTreeMap::new(),
        keep: false,
    };
    let observation =
        runner::run_exit_status(&program, &program_args, expect_exit, &work, &options)?;
    let _ = fs::remove_dir_all(&work);
    let outcome = Outcome::new(
        Case::new(
            program
                .file_name()
                .map_or_else(String::new, |name| name.to_string_lossy().into_owned()),
            program.display().to_string(),
            "run",
        ),
        observation.verdict,
        observation.reason,
        "-",
    );
    print!("{}", report::render(&[outcome], report::Format::Human));
    Ok(if observation.verdict == Verdict::Fail {
        EXIT_CHECK_FAILED
    } else {
        EXIT_OK
    })
}

fn command_gen_suite(args: &[String]) -> Result<u8> {
    let mut parser = ValueParser::new(args);
    let mut all = false;
    let mut suite = None;
    let mut oracle = PathBuf::from("tests/libc-oracle");
    let mut out = None;
    while let Some(flag) = parser.next() {
        match flag {
            "--all" => all = true,
            "--suite" => suite = Some(parser.value("--suite")?.to_string()),
            "--resources" => {
                let _ = parser.value("--resources")?;
            }
            "--vendor" => oracle = PathBuf::from(parser.value("--vendor")?),
            "--out" => out = Some(PathBuf::from(parser.value("--out")?)),
            other => bail!("unknown gen-suite flag `{other}`\n{USAGE}"),
        }
    }
    if all == suite.is_some() {
        bail!("gen-suite needs exactly one of --all and --suite\n{USAGE}");
    }
    let root = repo_root();
    let oracle = resolve(&root, &oracle.to_string_lossy());
    let out = out.map_or_else(
        || oracle.join("suites"),
        |path| resolve(&root, &path.to_string_lossy()),
    );
    let written = suite::generate(&oracle, &out, suite.as_deref(), &today())?;
    println!("gen-suite: {} suite(s) written", written.len());
    Ok(EXIT_OK)
}

fn command_suite(args: &[String]) -> Result<u8> {
    let mut parser = ValueParser::new(args);
    let root = repo_root();
    let mut name = None;
    let mut mode = None;
    let mut sysroot = None;
    let mut backend = RunBackend::None;
    let mut cc = std::env::var("CC").unwrap_or_else(|_| "cc".to_string());
    let mut out = PathBuf::from("build/libc-conformance");
    let mut filter = None;
    let mut jobs = default_jobs();
    let mut max_timeout = 120u64;
    let mut keep = false;
    let mut vars = BTreeMap::new();
    while let Some(flag) = parser.next() {
        match flag {
            "--suite" | "--resources" => {
                let value = parser.value(flag)?;
                if flag == "--suite" {
                    name = Some(value.to_string());
                }
            }
            "--mode" => {
                let value = parser.value("--mode")?;
                mode = Some(match value {
                    "oracle" => Mode::Oracle,
                    "target" => Mode::Target,
                    other => bail!("unknown mode `{other}`"),
                });
            }
            "--target" => {
                let value = parser.value("--target")?;
                if let Some(arch) = value.strip_prefix("qemu-") {
                    let _ = arch;
                    bail!("not implemented here (C73)");
                }
                mode = Some(match value {
                    "host-oracle" => Mode::Oracle,
                    "host-target" => Mode::Target,
                    other => bail!("unknown --target `{other}`"),
                });
            }
            "--sysroot" => sysroot = Some(PathBuf::from(parser.value("--sysroot")?)),
            "--run-backend" => {
                let value = parser.value("--run-backend")?;
                backend = RunBackend::parse(value)
                    .map_err(|error| anyhow::anyhow!("{error}\n{USAGE}"))?;
            }
            "--cc" => cc = parser.value("--cc")?.to_string(),
            "--out" => out = PathBuf::from(parser.value("--out")?),
            "--filter" => filter = Some(parser.value("--filter")?.to_string()),
            "--jobs" => jobs = parse_u64(parser.value("--jobs")?, "--jobs")? as usize,
            "--max-timeout" => {
                max_timeout = parse_u64(parser.value("--max-timeout")?, "--max-timeout")?
            }
            "--keep" => keep = true,
            "--var" => {
                let (key, value) = split_var(parser.value("--var")?)?;
                vars.insert(key, value);
            }
            other => bail!("unknown suite flag `{other}`\n{USAGE}"),
        }
    }
    let Some(name) = name else {
        bail!("suite needs --suite NAME\n{USAGE}");
    };
    let Some(mode) = mode else {
        bail!("suite needs --mode oracle|target\n{USAGE}");
    };
    if mode == Mode::Target && sysroot.is_none() {
        bail!("target mode needs --sysroot DIR\n{USAGE}");
    }
    if backend == RunBackend::HostLinux {
        let readme = root.join("libc/ravenlibc/crates/raven-libc-sys/README.md");
        let text = fs::read_to_string(&readme).unwrap_or_default();
        let has_backend = text
            .lines()
            .any(|line| line.contains("Linux") && line.contains("backend"));
        if !has_backend {
            bail!("host-linux: raven-libc-sys has no Linux backend (O-B5)");
        }
    }
    let oracle = root.join("tests/libc-oracle");
    let out = resolve(&root, &out.to_string_lossy());
    for suite_name in suite_names(&name) {
        let suite = suite::find(&oracle, &suite_name)?;
        let config = RunConfig {
            root: root.clone(),
            suite,
            mode,
            backend,
            cc: cc.clone(),
            out: out.clone(),
            filter: filter.clone(),
            jobs,
            max_timeout,
            keep,
            vars: vars.clone(),
            sysroot: sysroot
                .as_ref()
                .map(|path| resolve(&root, &path.to_string_lossy())),
        };
        suite::run(&config)?;
    }
    Ok(EXIT_OK)
}

fn suite_names(name: &str) -> Vec<String> {
    match name {
        "netbsd" => vec![
            "netbsd-libc".to_string(),
            "netbsd-libm".to_string(),
            "netbsd-libpthread".to_string(),
        ],
        "freebsd" => vec![
            "freebsd-libc".to_string(),
            "freebsd-msun".to_string(),
            "freebsd-libthr".to_string(),
        ],
        "openbsd" => vec![
            "openbsd-libc".to_string(),
            "openbsd-libm".to_string(),
            "openbsd-libpthread".to_string(),
        ],
        other => vec![other.to_string()],
    }
}

fn command_table(args: &[String]) -> Result<u8> {
    let root = repo_root();
    let mut parser = ValueParser::new(args);
    let mut write = false;
    let mut runs = table::default_runs(&root);
    let mut baseline = root.join("tests/libc/conformance.tsv");
    let mut markdown = root.join("tests/libc/CONFORMANCE.md");
    let mut accept = None;
    while let Some(flag) = parser.next() {
        match flag {
            "--write" => write = true,
            "--runs" => runs = resolve(&root, parser.value("--runs")?),
            "--baseline" => baseline = resolve(&root, parser.value("--baseline")?),
            "--md" => markdown = resolve(&root, parser.value("--md")?),
            "--accept-regressions" => {
                accept = Some(parser.value("--accept-regressions")?.to_string())
            }
            other => bail!("unknown table flag `{other}`\n{USAGE}"),
        }
    }
    let table = table::build_from_runs(&runs)?;
    if write {
        if baseline.is_file() {
            let old = table::read_raw(&baseline)?;
            let regressions = table::regressions(&old, &table)?;
            if !regressions.is_empty() && accept.is_none() {
                for (case, before, after) in regressions.iter().take(20) {
                    eprintln!(
                        "regression: {} {}/{} was {before}, now {after}",
                        case.suite, case.program, case.name
                    );
                }
                bail!(
                    "table --write refuses {} regression(s); pass --accept-regressions \"reason\"",
                    regressions.len()
                );
            }
        }
        table::write_baseline(&baseline, &table)?;
        table::write_markdown(&markdown, &table)?;
        println!(
            "table: wrote {} bodies to {} and {}",
            table.rows.len(),
            baseline.display(),
            markdown.display()
        );
    } else {
        print!("{}", table::render_markdown(&table));
    }
    Ok(EXIT_OK)
}

fn command_check(args: &[String]) -> Result<u8> {
    let root = repo_root();
    let mut parser = ValueParser::new(args);
    let mut baseline_path = root.join("tests/libc/conformance.tsv");
    let mut run_path = None;
    while let Some(flag) = parser.next() {
        match flag {
            "--table" => baseline_path = resolve(&root, parser.value("--table")?),
            "--run" => run_path = Some(resolve(&root, parser.value("--run")?)),
            other => bail!("unknown check flag `{other}`\n{USAGE}"),
        }
    }
    let baseline = table::read_raw(&baseline_path)
        .with_context(|| format!("cannot read baseline {}", baseline_path.display()))?;
    let run = if let Some(path) = run_path {
        if path.is_dir() && (path.join("oracle").is_dir() || path.join("target").is_dir()) {
            rows_of(&table::build_from_runs(&path)?)
        } else if path.is_dir() {
            collect_tsv(&path)?
        } else {
            table::read_raw(&path)?
        }
    } else {
        let runs = table::default_runs(&root);
        if runs.is_dir() {
            let table = table::build_from_runs(&runs)?;
            rows_of(&table)
        } else {
            Vec::new()
        }
    };
    let report = check::check(&baseline, &run)?;
    print_check(&report);
    Ok(if report.failed() {
        EXIT_CHECK_FAILED
    } else {
        EXIT_OK
    })
}

fn command_gate(args: &[String]) -> Result<u8> {
    let root = repo_root();
    let mut parser = ValueParser::new(args);
    let mut g1 = false;
    let mut table_path = root.join("tests/libc/conformance.tsv");
    let mut config_path = root.join("tests/libc-oracle/g1.toml");
    while let Some(flag) = parser.next() {
        match flag {
            "--g1" => g1 = true,
            "--table" => table_path = resolve(&root, parser.value("--table")?),
            "--config" => config_path = resolve(&root, parser.value("--config")?),
            other => bail!("unknown gate flag `{other}`\n{USAGE}"),
        }
    }
    if !g1 {
        bail!("gate needs --g1\n{USAGE}");
    }
    let config = GateConfig::load(&config_path)?;
    let raw = table::read_raw(&table_path)
        .with_context(|| format!("cannot read {}", table_path.display()))?;
    let rows: Vec<TableRow> = raw
        .iter()
        .filter(|row| row.verdict != "removed")
        .map(|row| {
            let outcome = row.outcome()?;
            let oracle_pass = outcome.verdict == Verdict::Pass;
            Ok(TableRow {
                outcome,
                oracle_pass,
            })
        })
        .collect::<Result<Vec<_>>>()?;
    let table = ConformanceTable {
        rows,
        stats: Vec::new(),
    };
    let gate_rows = gate::evaluate(&config, &table);
    print!("{}", gate::render(&gate_rows));
    Ok(if gate::met(&gate_rows) {
        EXIT_OK
    } else {
        EXIT_GATE_NOT_MET
    })
}

fn rows_of(table: &ConformanceTable) -> Vec<RawRow> {
    table
        .rows
        .iter()
        .map(|row| RawRow {
            suite: row.outcome.case.suite.clone(),
            program: row.outcome.case.program.clone(),
            case: row.outcome.case.name.clone(),
            verdict: row.outcome.verdict.as_str().to_string(),
            reason: row.outcome.reason.clone(),
            ticket: row.outcome.ticket.clone(),
        })
        .collect()
}

fn collect_tsv(directory: &Path) -> Result<Vec<RawRow>> {
    let mut rows = Vec::new();
    for entry in
        fs::read_dir(directory).with_context(|| format!("cannot read {}", directory.display()))?
    {
        let entry = entry.context("cannot read a run entry")?;
        let path = entry.path();
        if path.is_dir() {
            rows.extend(collect_tsv(&path)?);
        } else if path.extension().is_some_and(|extension| extension == "tsv")
            && !path.to_string_lossy().contains(".builds.")
        {
            rows.extend(table::read_raw(&path)?);
        }
    }
    rows.sort_by(|left, right| {
        (&left.suite, &left.program, &left.case).cmp(&(&right.suite, &right.program, &right.case))
    });
    Ok(rows)
}

fn print_check(report: &CheckReport) {
    for (case, verdict) in &report.regressions {
        println!(
            "REGRESSION {} {}/{}: pass -> {verdict}",
            case.suite, case.program, case.name
        );
    }
    for case in &report.vanished {
        println!(
            "VANISHED {} {}/{}: no result and no removed marker",
            case.suite, case.program, case.name
        );
    }
    for case in &report.unlisted {
        println!(
            "UNLISTED {} {}/{}: not in the baseline",
            case.suite, case.program, case.name
        );
    }
    println!(
        "check: {} regression(s), {} vanished, {} unlisted, improved {}",
        report.regressions.len(),
        report.vanished.len(),
        report.unlisted.len(),
        report.improvements.len()
    );
}

fn parse_u64(value: &str, flag: &str) -> Result<u64> {
    value
        .parse()
        .map_err(|_| anyhow::anyhow!("{flag} needs a non-negative integer\n{USAGE}"))
}

fn split_var(value: &str) -> Result<(String, String)> {
    value
        .split_once('=')
        .map(|(key, value)| (key.to_string(), value.to_string()))
        .ok_or_else(|| anyhow::anyhow!("--var needs K=V\n{USAGE}"))
}

fn default_jobs() -> usize {
    std::thread::available_parallelism()
        .map_or(1, std::num::NonZeroUsize::get)
        .min(8)
}

fn today() -> String {
    let seconds = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_secs());
    let days = i64::try_from(seconds / 86_400).unwrap_or(0);
    let (year, month, day) = civil_from_days(days);
    format!("{year:04}-{month:02}-{day:02}")
}

fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let year = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    (
        if month <= 2 { year + 1 } else { year },
        month as u32,
        day as u32,
    )
}

#[cfg(test)]
#[path = "main_ut.rs"]
mod tests;
