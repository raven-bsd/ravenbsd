// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `linux-corpus` command-line interface.

#![forbid(unsafe_code)]

use std::collections::HashSet;
use std::env;
use std::path::PathBuf;
use std::process::ExitCode;

use anyhow::{Result, anyhow, bail};
use linux_corpus::merge::{MergeOptions, merge, missing, read_implemented};
use linux_corpus::runner::{RunOptions, run_corpus};

const USAGE: &str = "\
usage:
  linux-corpus run [--corpus tools/linux-corpus/corpus.toml] [--out tools/linux-corpus/out] [--only <id>...]
  linux-corpus merge [--out tools/linux-corpus/out] [--json docs/linux-syscall-coverage.json]
                     [--md docs/linux-syscall-coverage.md] [--implemented <file>]
  linux-corpus missing --implemented <file> [--out tools/linux-corpus/out]";

fn main() -> ExitCode {
    match run() {
        Ok(code) => ExitCode::from(code),
        Err(error) => {
            eprintln!("linux-corpus: {error:#}");
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
        "run" => run_corpus_command(&args[1..]),
        "merge" => merge_command(&args[1..]),
        "missing" => missing_command(&args[1..]),
        "-h" | "--help" | "help" => {
            println!("{USAGE}");
            Ok(0)
        }
        other => bail!("unknown command `{other}`\n{USAGE}"),
    }
}

fn run_corpus_command(args: &[String]) -> Result<u8> {
    let mut options = RunOptions {
        corpus: PathBuf::from("tools/linux-corpus/corpus.toml"),
        out: PathBuf::from("tools/linux-corpus/out"),
        only: Vec::new(),
    };
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--corpus" => {
                index += 1;
                options.corpus = PathBuf::from(value(args, index, "--corpus")?);
            }
            "--out" => {
                index += 1;
                options.out = PathBuf::from(value(args, index, "--out")?);
            }
            "--only" => {
                let start = index + 1;
                let mut end = start;
                while end < args.len() && !args[end].starts_with("--") {
                    end += 1;
                }
                if end == start {
                    bail!("--only needs at least one program id\n{USAGE}");
                }
                options.only.extend(args[start..end].iter().cloned());
                index = end;
                continue;
            }
            other => bail!("unexpected argument `{other}`\n{USAGE}"),
        }
        index += 1;
    }

    let results = run_corpus(&options)?;
    println!("id\tcategory\tstatus\texit\tsyscalls\tduration_ms\tdetail");
    for result in &results {
        let exit = result
            .exit
            .map_or_else(|| "-".to_string(), |value| value.to_string());
        println!(
            "{}\t{}\t{}\t{}\t{}\t{}\t{}",
            result.id,
            result.category,
            result.status(),
            exit,
            result.syscalls_used,
            result.duration_ms,
            result.reason.clone().unwrap_or_default()
        );
    }
    let skipped = results.iter().filter(|result| result.skipped).count();
    let failed = results
        .iter()
        .filter(|result| !result.is_ok() && !result.skipped)
        .count();
    eprintln!(
        "{} program(s): {} ok, {} skipped, {} not passing",
        results.len(),
        results.iter().filter(|result| result.is_ok()).count(),
        skipped,
        failed
    );
    Ok(0)
}

fn merge_command(args: &[String]) -> Result<u8> {
    let mut options = MergeOptions {
        out: PathBuf::from("tools/linux-corpus/out"),
        json: PathBuf::from("docs/linux-syscall-coverage.json"),
        md: PathBuf::from("docs/linux-syscall-coverage.md"),
        implemented: None,
    };
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--out" => {
                index += 1;
                options.out = PathBuf::from(value(args, index, "--out")?);
            }
            "--json" => {
                index += 1;
                options.json = PathBuf::from(value(args, index, "--json")?);
            }
            "--md" => {
                index += 1;
                options.md = PathBuf::from(value(args, index, "--md")?);
            }
            "--implemented" => {
                index += 1;
                options.implemented = Some(PathBuf::from(value(args, index, "--implemented")?));
            }
            other => bail!("unexpected argument `{other}`\n{USAGE}"),
        }
        index += 1;
    }
    let coverage = merge(&options)?;
    eprintln!(
        "wrote {} programs and {} syscalls to {} and {}",
        coverage.programs.len(),
        coverage.syscalls.len(),
        options.json.display(),
        options.md.display()
    );
    Ok(0)
}

fn missing_command(args: &[String]) -> Result<u8> {
    let mut out = PathBuf::from("tools/linux-corpus/out");
    let mut implemented = None;
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--out" => {
                index += 1;
                out = PathBuf::from(value(args, index, "--out")?);
            }
            "--implemented" => {
                index += 1;
                implemented = Some(PathBuf::from(value(args, index, "--implemented")?));
            }
            other => bail!("unexpected argument `{other}`\n{USAGE}"),
        }
        index += 1;
    }
    let implemented = implemented.ok_or_else(|| anyhow!("--implemented is required\n{USAGE}"))?;
    let implemented: HashSet<String> = read_implemented(&implemented)?;
    for syscall in missing(&out, &implemented)? {
        println!(
            "{}\t{}\t{}",
            syscall.name, syscall.program_count, syscall.total_calls
        );
    }
    Ok(0)
}

fn value<'a>(args: &'a [String], index: usize, flag: &str) -> Result<&'a str> {
    args.get(index)
        .map(String::as_str)
        .ok_or_else(|| anyhow!("{flag} needs a value"))
}
