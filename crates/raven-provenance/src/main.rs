// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Command-line interface for the RavenBSD provenance checker.

#![forbid(unsafe_code)]

use std::env;
use std::path::PathBuf;
use std::process::ExitCode;

use anyhow::{Result, anyhow, bail};
use raven_provenance::{CheckOptions, check, fix_spdx, list};

const USAGE: &str = "\
usage:
  raven-provenance check [--root <dir>] [--json] [--donors-lock <path>] [--strict]
  raven-provenance list [--root <dir>]
  raven-provenance fix-spdx [--root <dir>]

`--strict` makes a dependency scan that cannot run (`cargo metadata --offline` fails or
returns no package list) a violation instead of a warning; the gate uses it.";

fn main() -> ExitCode {
    match run() {
        Ok(code) => ExitCode::from(code),
        Err(error) => {
            eprintln!("raven-provenance: {error:#}");
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
        "check" => run_check(&args[1..]),
        "list" => {
            let root = parse_root(&args[1..])?;
            for (path, license) in list(&root)? {
                println!("{path}\t{license}");
            }
            Ok(0)
        }
        "fix-spdx" => {
            let root = parse_root(&args[1..])?;
            let fixed = fix_spdx(&root)?;
            for path in &fixed {
                println!("fixed: {path}");
            }
            eprintln!("{} file(s) updated", fixed.len());
            Ok(0)
        }
        "-h" | "--help" | "help" => {
            println!("{USAGE}");
            Ok(0)
        }
        other => bail!("unknown command `{other}`\n{USAGE}"),
    }
}

fn run_check(args: &[String]) -> Result<u8> {
    let mut options = CheckOptions::new(".");
    let mut json = false;
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--root" => {
                index += 1;
                options.root = PathBuf::from(value(args, index, "--root")?);
            }
            "--donors-lock" => {
                index += 1;
                options.donors_lock = Some(PathBuf::from(value(args, index, "--donors-lock")?));
            }
            "--json" => json = true,
            "--strict" => options.strict = true,
            other => bail!("unexpected argument `{other}`\n{USAGE}"),
        }
        index += 1;
    }
    let report = check(&options)?;
    if json {
        println!("{}", serde_json::to_string(&report)?);
    } else {
        for violation in &report.violations {
            println!(
                "{}: {}: {}",
                violation.path, violation.rule, violation.message
            );
        }
        eprintln!(
            "{} file(s) checked, {} violation(s), {} warning(s)",
            report.files_checked,
            report.violations.len(),
            report.warnings.len()
        );
        for warning in &report.warnings {
            eprintln!("warning: {warning}");
        }
    }
    Ok(u8::from(!report.violations.is_empty()))
}

fn parse_root(args: &[String]) -> Result<PathBuf> {
    let mut root = PathBuf::from(".");
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--root" => {
                index += 1;
                root = PathBuf::from(value(args, index, "--root")?);
            }
            other => bail!("unexpected argument `{other}`\n{USAGE}"),
        }
        index += 1;
    }
    Ok(root)
}

fn value<'a>(args: &'a [String], index: usize, flag: &str) -> Result<&'a str> {
    args.get(index)
        .map(String::as_str)
        .ok_or_else(|| anyhow!("{flag} needs a value"))
}
