// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `libc-gate` command-line entry point.

#![forbid(unsafe_code)]

use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::Duration;

use anyhow::{Context, Result, bail};

use libc_gate::eval::{self, Options};
use libc_gate::man;
use libc_gate::model::Status;
use libc_gate::report;
use libc_gate::rows::Rows;

const USAGE: &str = "\
usage:
  libc-gate rows   [--rows FILE]
  libc-gate eval   [--rows FILE] [--only ID] [--out DIR] [--timeout-secs N] [--repo DIR]
  libc-gate table  [--rows FILE] [--out DIR]
  libc-gate man-list  [--symbols FILE]
  libc-gate man-check [--symbols FILE] [--pages FILE] [--man DIR] [--repo DIR]
  libc-gate verdict [--rows FILE] [--out DIR] [--repo DIR]
  libc-gate help";

#[derive(Debug, Default)]
struct Args {
    rows: Option<PathBuf>,
    out: Option<PathBuf>,
    only: Option<String>,
    timeout: Option<u64>,
    repo: Option<PathBuf>,
    symbols: Option<PathBuf>,
    pages: Option<PathBuf>,
    man: Option<PathBuf>,
}

fn default_rows(repo: &Path) -> PathBuf {
    repo.join("tests/libc/gate/rows.toml")
}

fn main() -> ExitCode {
    match run(std::env::args().skip(1).collect()) {
        Ok(code) => ExitCode::from(code),
        Err(error) => {
            eprintln!("error: {error:#}");
            ExitCode::from(2)
        }
    }
}

fn run(args: Vec<String>) -> Result<u8> {
    let Some(command) = args.first() else {
        println!("{USAGE}");
        return Ok(0);
    };
    if matches!(command.as_str(), "-h" | "--help" | "help") {
        println!("{USAGE}");
        return Ok(0);
    }
    let parsed = parse(&args[1..])?;
    let repo = parsed
        .repo
        .clone()
        .or_else(|| std::env::current_dir().ok())
        .unwrap_or_else(|| PathBuf::from("."));
    match command.as_str() {
        "rows" => command_rows(&repo, &parsed),
        "eval" => command_eval(&repo, &parsed),
        "table" => command_table(&repo, &parsed, true),
        "man-list" => command_man_list(&repo, &parsed),
        "man-check" => command_man_check(&repo, &parsed),
        "verdict" => command_verdict(&repo, &parsed),
        other => bail!("unknown command `{other}`\n{USAGE}"),
    }
}

fn parse(args: &[String]) -> Result<Args> {
    let mut parsed = Args::default();
    let mut index = 0;
    while index < args.len() {
        let flag = args[index].as_str();
        let value = |index: usize| -> Result<&str> {
            args.get(index)
                .map(String::as_str)
                .ok_or_else(|| anyhow::anyhow!("{flag} needs a value"))
        };
        match flag {
            "--rows" => {
                index += 1;
                parsed.rows = Some(PathBuf::from(value(index)?));
            }
            "--out" => {
                index += 1;
                parsed.out = Some(PathBuf::from(value(index)?));
            }
            "--only" => {
                index += 1;
                parsed.only = Some(value(index)?.to_string());
            }
            "--timeout-secs" => {
                index += 1;
                parsed.timeout = Some(value(index)?.parse().context("timeout seconds")?);
            }
            "--repo" => {
                index += 1;
                parsed.repo = Some(PathBuf::from(value(index)?));
            }
            "--symbols" => {
                index += 1;
                parsed.symbols = Some(PathBuf::from(value(index)?));
            }
            "--pages" => {
                index += 1;
                parsed.pages = Some(PathBuf::from(value(index)?));
            }
            "--man" => {
                index += 1;
                parsed.man = Some(PathBuf::from(value(index)?));
            }
            other => bail!("unknown argument `{other}`\n{USAGE}"),
        }
        index += 1;
    }
    Ok(parsed)
}

fn command_rows(repo: &Path, args: &Args) -> Result<u8> {
    let rows = Rows::load(&args.rows.clone().unwrap_or_else(|| default_rows(repo)))?;
    for check in &rows.check {
        println!(
            "{}\t{}\t{}\t{}\t{}",
            check.id,
            check.row,
            if check.required { "required" } else { "info" },
            check.kind,
            check.title
        );
    }
    Ok(0)
}

fn command_eval(repo: &Path, args: &Args) -> Result<u8> {
    let rows = Rows::load(&args.rows.clone().unwrap_or_else(|| default_rows(repo)))?;
    let mut options = Options {
        repo: repo.to_path_buf(),
        ..Options::default()
    };
    if let Some(out) = &args.out {
        options.out = out.clone();
    }
    if let Some(seconds) = args.timeout {
        options.timeout = Duration::from_secs(seconds);
    }
    options.only = args.only.clone();
    let results = eval::eval(&rows, &options)?;
    println!("libc-gate: evaluated {} check(s)", results.len());
    Ok(0)
}

fn command_table(repo: &Path, args: &Args, print: bool) -> Result<u8> {
    let rows = Rows::load(&args.rows.clone().unwrap_or_else(|| default_rows(repo)))?;
    let out = args
        .out
        .clone()
        .unwrap_or_else(|| repo.join("build/libc-gate"));
    let results = report::load_results(&out)?;
    let table = report::render(&results, &rows);
    if print {
        println!("{table}");
    }
    Ok(0)
}

fn command_man_list(repo: &Path, args: &Args) -> Result<u8> {
    let symbols = args
        .symbols
        .clone()
        .unwrap_or_else(|| repo.join("libc/ravenlibc/abi/census/symbols.toml"));
    let parsed = man::load_symbols(&symbols)?;
    print!("{}", man::list_candidates(&parsed));
    Ok(0)
}

fn command_man_check(repo: &Path, args: &Args) -> Result<u8> {
    let symbols = args
        .symbols
        .clone()
        .unwrap_or_else(|| repo.join("libc/ravenlibc/abi/census/symbols.toml"));
    let pages = args
        .pages
        .clone()
        .unwrap_or_else(|| repo.join("tests/libc/gate/man-pages.toml"));
    let docs = args.man.clone().unwrap_or_else(|| repo.join("docs"));
    let report = man::check(repo, &symbols, &pages, &docs)?;
    if report.is_ok() {
        println!(
            "man-check: ok ({} candidates, {} pages, {} covered)",
            report.candidates.len(),
            report.pages,
            report.covered
        );
        Ok(0)
    } else {
        for error in &report.errors {
            eprintln!("man-check: {error}");
        }
        Ok(1)
    }
}

fn command_verdict(repo: &Path, args: &Args) -> Result<u8> {
    let rows = Rows::load(&args.rows.clone().unwrap_or_else(|| default_rows(repo)))?;
    let out = args
        .out
        .clone()
        .unwrap_or_else(|| repo.join("build/libc-gate"));
    let results = report::load_results(&out)?;
    let (status, _) = report::verdict_line(&results, &rows);
    println!("libc gate: {status}");
    println!("{}", report::render(&results, &rows));
    Ok(if status == Status::Met { 0 } else { 1 })
}

#[cfg(test)]
#[path = "main_ut.rs"]
mod tests;
