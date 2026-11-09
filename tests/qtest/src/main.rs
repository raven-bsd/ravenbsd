// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `raven-qtest` command-line interface.

#![forbid(unsafe_code)]

use std::env;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use anyhow::{Result, anyhow, bail};
use raven_qtest::{
    Case, CaseOutcome, ExecuteOptions, ResultKind, execute, is_valid_arch, list_case_files,
    write_outcome,
};

const USAGE: &str = "\
usage:
  raven-qtest run --case <file.toml> [--image <img>] [--arch <a>] [--out <dir>]
                  [--ovmf-code <p>] [--ovmf-vars <p>] [--runner <command...>]
  raven-qtest list [--cases <dir>]
  raven-qtest all [--arch <a>] [--no-build]";

fn main() -> ExitCode {
    match run() {
        Ok(code) => ExitCode::from(code),
        Err(error) => {
            eprintln!("raven-qtest: {error:#}");
            ExitCode::from(2)
        }
    }
}

fn run() -> Result<u8> {
    let args: Vec<String> = env::args().skip(1).collect();
    let Some(command) = args.first().map(String::as_str) else {
        bail!("missing command\n{USAGE}");
    };
    match command {
        "run" => run_one(&args[1..]),
        "list" => list(&args[1..]),
        "all" => run_all(&args[1..]),
        "-h" | "--help" | "help" => {
            println!("{USAGE}");
            Ok(0)
        }
        other => bail!("unknown command `{other}`\n{USAGE}"),
    }
}

fn run_one(args: &[String]) -> Result<u8> {
    let mut case_path = None;
    let mut options = ExecuteOptions::new();
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--case" => {
                index += 1;
                case_path = Some(PathBuf::from(value(args, index, "--case")?));
            }
            "--image" => {
                index += 1;
                options.image = Some(PathBuf::from(value(args, index, "--image")?));
            }
            "--arch" => {
                index += 1;
                options.arch = Some(value(args, index, "--arch")?.to_string());
            }
            "--out" => {
                index += 1;
                options.out_dir = PathBuf::from(value(args, index, "--out")?);
            }
            "--ovmf-code" => {
                index += 1;
                options.ovmf_code = Some(PathBuf::from(value(args, index, "--ovmf-code")?));
            }
            "--ovmf-vars" => {
                index += 1;
                options.ovmf_vars = Some(PathBuf::from(value(args, index, "--ovmf-vars")?));
            }
            "--runner" => {
                let runner = args[index + 1..].to_vec();
                if runner.is_empty() {
                    bail!("--runner needs a command\n{USAGE}");
                }
                options.runner = Some(runner);
                index = args.len();
                continue;
            }
            other => bail!("unexpected argument `{other}`\n{USAGE}"),
        }
        index += 1;
    }
    let case_path = case_path.ok_or_else(|| anyhow!("--case is required\n{USAGE}"))?;
    if let Some(arch) = &options.arch
        && !is_valid_arch(arch)
    {
        bail!("arch must be one of x86_64, aarch64, riscv64\n{USAGE}");
    }
    let case = Case::load(&case_path)?;
    let outcome = execute(&case, &options);
    write_outcome(&outcome, &options.out_dir)?;
    print_outcome(&outcome, true);
    Ok(match outcome.report.result {
        ResultKind::Pass => 0,
        ResultKind::Skipped => 77,
        _ => 1,
    })
}

fn list(args: &[String]) -> Result<u8> {
    let mut cases_dir = PathBuf::from("tests/qtest/cases");
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--cases" => {
                index += 1;
                cases_dir = PathBuf::from(value(args, index, "--cases")?);
            }
            other => bail!("unexpected argument `{other}`\n{USAGE}"),
        }
        index += 1;
    }
    for file in list_case_files(&cases_dir)? {
        let case = Case::load(&file)?;
        println!("{}\t{}", case.spec.name, file.display());
    }
    Ok(0)
}

fn run_all(args: &[String]) -> Result<u8> {
    let mut arch = "x86_64".to_string();
    let mut no_build = false;
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--arch" => {
                index += 1;
                arch = value(args, index, "--arch")?.to_string();
            }
            "--no-build" => no_build = true,
            other => bail!("unexpected argument `{other}`\n{USAGE}"),
        }
        index += 1;
    }
    if !is_valid_arch(&arch) {
        bail!("arch must be one of x86_64, aarch64, riscv64\n{USAGE}");
    }

    let mut executed = 0;
    let mut failed = 0;
    for file in list_case_files(Path::new("tests/qtest/cases"))? {
        let case = Case::load(&file)?;
        if case.spec.arch != arch {
            continue;
        }
        let mut options = ExecuteOptions::new();
        options.no_build = no_build;
        let outcome = execute(&case, &options);
        write_outcome(&outcome, &options.out_dir)?;
        print_outcome(&outcome, true);
        executed += 1;
        if !outcome.report.result.is_success() {
            failed += 1;
        }
    }
    if executed == 0 {
        println!("no cases for arch {arch}");
    }
    Ok(u8::from(failed > 0))
}

fn print_outcome(outcome: &CaseOutcome, print_detail: bool) {
    println!(
        "{}: {}",
        outcome.report.case,
        outcome.report.result.as_str()
    );
    if print_detail
        && let Some(detail) = &outcome.detail
        && matches!(
            outcome.report.result,
            ResultKind::Skipped | ResultKind::Error
        )
    {
        println!("  {detail}");
    }
    if matches!(
        outcome.report.result,
        ResultKind::Fail | ResultKind::Timeout | ResultKind::Crash
    ) {
        eprintln!("--- serial (last 40 lines) ---");
        let start = outcome.lines.len().saturating_sub(40);
        for line in &outcome.lines[start..] {
            eprintln!("{line}");
        }
        if !outcome.report.unmatched.is_empty() {
            eprintln!("--- unmatched expectations ---");
            for regex in &outcome.report.unmatched {
                eprintln!("{regex}");
            }
        }
        if !outcome.report.forbidden_hits.is_empty() {
            eprintln!("--- forbidden hits ---");
            for line in &outcome.report.forbidden_hits {
                eprintln!("{line}");
            }
        }
        if let Some(detail) = &outcome.detail {
            eprintln!("--- result ---");
            eprintln!("{detail}");
        }
    }
}

fn value<'a>(args: &'a [String], index: usize, flag: &str) -> Result<&'a str> {
    args.get(index)
        .map(String::as_str)
        .ok_or_else(|| anyhow!("{flag} needs a value"))
}
