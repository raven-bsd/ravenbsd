// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `raven-conform` command-line interface.

#![forbid(unsafe_code)]

use std::env;
use std::fs;
use std::process;

use anyhow::{Context, Result, bail};
use raven_conform::compare::compare;
use raven_conform::known::KnownFailures;
use raven_conform::result::Results;
use raven_conform::summary::{json, markdown, summarize};

fn main() {
    let code = match run() {
        Ok(code) => code,
        Err(error) => {
            eprintln!("raven-conform: {error:#}");
            1
        }
    };
    process::exit(code);
}

fn run() -> Result<i32> {
    let mut args = env::args().skip(1);
    let Some(command) = args.next() else {
        print_usage();
        return Ok(2);
    };
    match command.as_str() {
        "parse" => parse_command(args.collect()),
        "compare" => compare_command(args.collect()),
        "summary" => summary_command(args.collect()),
        "--help" | "-h" | "help" => {
            print_usage();
            Ok(0)
        }
        other => bail!("unknown command `{other}`; run `raven-conform --help`"),
    }
}

fn parse_command(args: Vec<String>) -> Result<i32> {
    let mut tool = None;
    let mut file = None;
    let mut target = None;
    let mut args = args.into_iter();
    while let Some(argument) = args.next() {
        match argument.as_str() {
            "--target" => {
                target = Some(args.next().context("--target needs a value")?);
            }
            other if tool.is_none() => tool = Some(other.to_string()),
            other if file.is_none() => file = Some(other.to_string()),
            other => bail!("unexpected argument `{other}`"),
        }
    }
    let tool = tool.context("parse needs a tool name (pjdfstest or ltp)")?;
    let file = file.context("parse needs an input file")?;
    let input = fs::read_to_string(&file).with_context(|| format!("cannot read `{file}`"))?;
    let results = match tool.as_str() {
        "pjdfstest" => raven_conform::pjdfstest::parse(&input, target.as_deref()),
        "ltp" => raven_conform::ltp::parse(&input, target.as_deref()),
        other => bail!("unknown tool `{other}`; expected pjdfstest or ltp"),
    };
    println!("{}", serde_json::to_string_pretty(&results)?);
    Ok(0)
}

fn compare_command(args: Vec<String>) -> Result<i32> {
    let mut known = None;
    let mut results = None;
    let mut allow_new = false;
    let mut args = args.into_iter();
    while let Some(argument) = args.next() {
        match argument.as_str() {
            "--known" => known = Some(args.next().context("--known needs a file")?),
            "--allow-new" => allow_new = true,
            other if results.is_none() => results = Some(other.to_string()),
            other => bail!("unexpected argument `{other}`"),
        }
    }
    let known = known.context("compare needs --known <KNOWN_FAILURES>")?;
    let results = results.context("compare needs a results JSON file")?;
    let known_text =
        fs::read_to_string(&known).with_context(|| format!("cannot read `{known}`"))?;
    let known = KnownFailures::parse(&known_text)?;
    let results_text =
        fs::read_to_string(&results).with_context(|| format!("cannot read `{results}`"))?;
    let results: Results = serde_json::from_str(&results_text)
        .with_context(|| format!("invalid results JSON `{results}`"))?;

    let comparison = compare(&results, &known, allow_new);
    if allow_new {
        eprintln!("WARNING: the lead refuses additions without justification");
    }
    for result in &comparison.regressions {
        println!(
            "REGRESSION {}:{} ({})",
            result.suite,
            result.name,
            result.status.as_str()
        );
        if let Some(message) = &result.message {
            println!("  {message}");
        }
    }
    for result in &comparison.new_known {
        println!(
            "NEW-KNOWN {}:{} ({})",
            result.suite,
            result.name,
            result.status.as_str()
        );
    }
    for result in &comparison.fixed {
        println!(
            "FIXED {}:{}: remove from KNOWN_FAILURES",
            result.suite, result.name
        );
    }
    Ok(comparison.exit_code(allow_new))
}

fn summary_command(args: Vec<String>) -> Result<i32> {
    let mut json_output = false;
    let mut files = Vec::new();
    for argument in args {
        match argument.as_str() {
            "--json" => json_output = true,
            other if other.starts_with('-') => bail!("unexpected argument `{other}`"),
            other => files.push(other.to_string()),
        }
    }
    if files.is_empty() {
        bail!("summary needs at least one results JSON file");
    }
    let mut results = Vec::with_capacity(files.len());
    for file in &files {
        let text = fs::read_to_string(file).with_context(|| format!("cannot read `{file}`"))?;
        results.push(
            serde_json::from_str::<Results>(&text)
                .with_context(|| format!("invalid results JSON `{file}`"))?,
        );
    }
    let rows = summarize(&results);
    if json_output {
        println!("{}", json(&rows)?);
    } else {
        print!("{}", markdown(&rows));
    }
    Ok(0)
}

fn print_usage() {
    eprintln!(
        "usage:\n\
         \x20 raven-conform parse pjdfstest <file> [--target <name>]\n\
         \x20 raven-conform parse ltp <file> [--target <name>]\n\
         \x20 raven-conform compare --known <KNOWN_FAILURES> <results.json> [--allow-new]\n\
         \x20 raven-conform summary <results.json>... [--json]"
    );
}
