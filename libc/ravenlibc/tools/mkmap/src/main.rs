// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//!
//! `raven-libc-mkmap` command line: `merge`, `check` and `freeze`.
//!
//! Exit codes: 0 success, 1 a rule failed, 2 usage or I/O, 77 a tool is missing.

use std::path::PathBuf;
use std::process::ExitCode;

use anyhow::{Context, Result, bail};
use raven_libc_mkmap::check::{CheckOptions, run as check_run};
use raven_libc_mkmap::merge::merge;
use raven_libc_mkmap::tools::MissingTool;
use raven_libc_mkmap::versions::{parse_symbol_map, parse_versions};

/// Usage text.
const USAGE: &str = "\
usage:
  raven-libc-mkmap merge  --versions F --map F... --arch A --out-map F --out-list F
  raven-libc-mkmap check  --archive F --list F --map F... --versions F --hooks F \\
                          --expected DIR --arch A [--so F]
  raven-libc-mkmap freeze --so F --node N --out DIR [--arch A]";

fn main() -> ExitCode {
    match run(std::env::args().skip(1).collect()) {
        Ok(code) => ExitCode::from(code),
        Err(error) => {
            if let Some(missing) = error.downcast_ref::<MissingTool>() {
                eprintln!("skipped: {missing}");
                return ExitCode::from(77);
            }
            eprintln!("{error:#}");
            ExitCode::from(2)
        }
    }
}

fn run(args: Vec<String>) -> Result<u8> {
    let Some(command) = args.first() else {
        eprintln!("{USAGE}");
        bail!("missing command");
    };
    let rest = args.get(1..).unwrap_or(&[]);
    match command.as_str() {
        "merge" => run_merge(rest),
        "check" => run_check(rest),
        "freeze" => run_freeze(rest),
        "-h" | "--help" | "help" => {
            println!("{USAGE}");
            Ok(0)
        }
        value => {
            eprintln!("{USAGE}");
            bail!("unknown command `{value}`");
        }
    }
}

/// Parsed `--flag value` arguments: repeated flags keep every value.
type Flags = std::collections::BTreeMap<String, Vec<String>>;

/// Parse `--flag value` pairs into a map, keeping repeated `--map` values.
fn parse_flags(args: &[String]) -> Result<(Flags, Vec<String>)> {
    let mut flags: Flags = Flags::new();
    let mut positionals = Vec::new();
    let mut index = 0usize;
    while index < args.len() {
        let arg = args.get(index).context("argument index")?;
        if let Some(name) = arg.strip_prefix("--") {
            let value = args
                .get(index + 1)
                .with_context(|| format!("--{name} needs a value"))?;
            flags
                .entry(name.to_string())
                .or_default()
                .push(value.clone());
            index += 2;
        } else {
            positionals.push(arg.clone());
            index += 1;
        }
    }
    Ok((flags, positionals))
}

fn one<'a>(
    flags: &'a std::collections::BTreeMap<String, Vec<String>>,
    name: &str,
) -> Result<&'a str> {
    flags
        .get(name)
        .and_then(|values| values.first())
        .map(String::as_str)
        .with_context(|| format!("missing --{name}"))
}

fn many<'a>(
    flags: &'a std::collections::BTreeMap<String, Vec<String>>,
    name: &str,
) -> Result<&'a [String]> {
    flags
        .get(name)
        .map(Vec::as_slice)
        .with_context(|| format!("missing --{name}"))
}

fn run_merge(args: &[String]) -> Result<u8> {
    let (flags, positionals) = parse_flags(args)?;
    if !positionals.is_empty() {
        bail!("merge takes no positional argument");
    }
    let versions_path = PathBuf::from(one(&flags, "versions")?);
    let arch = one(&flags, "arch")?;
    let out_map = PathBuf::from(one(&flags, "out-map")?);
    let out_list = PathBuf::from(one(&flags, "out-list")?);
    let versions_text = std::fs::read_to_string(&versions_path)
        .with_context(|| format!("cannot read {}", versions_path.display()))?;
    let versions = parse_versions(&versions_text)?;
    let mut bodies = Vec::new();
    for path in many(&flags, "map")? {
        let path = PathBuf::from(path);
        let text = std::fs::read_to_string(&path)
            .with_context(|| format!("cannot read {}", path.display()))?;
        bodies.extend(parse_symbol_map(&text, &path.display().to_string())?);
    }
    let merged = merge(&versions, &bodies, arch)?;
    raven_libc_mkmap::list::write(&out_map, &merged.map)?;
    raven_libc_mkmap::list::write(&out_list, &merged.list)?;
    println!(
        "mkmap: merge: ok ({} public, {} private)",
        merged.public.len(),
        merged.private.len()
    );
    Ok(0)
}

fn run_check(args: &[String]) -> Result<u8> {
    let (flags, positionals) = parse_flags(args)?;
    if !positionals.is_empty() {
        bail!("check takes no positional argument");
    }
    let arch = one(&flags, "arch")?.to_string();
    let options = CheckOptions {
        root: std::env::current_dir().context("current directory")?,
        archive: PathBuf::from(one(&flags, "archive")?),
        list: PathBuf::from(one(&flags, "list")?),
        maps: many(&flags, "map")?.iter().map(PathBuf::from).collect(),
        versions: PathBuf::from(one(&flags, "versions")?),
        hooks: PathBuf::from(one(&flags, "hooks")?),
        expected: PathBuf::from(one(&flags, "expected")?),
        arch,
        so: flags
            .get("so")
            .and_then(|values| values.first())
            .map(PathBuf::from),
        nm: raven_libc_mkmap::tools::nm_program(),
        readelf: raven_libc_mkmap::tools::readelf_program(),
    };
    let (findings, summary) = check_run(&options)?;
    for finding in &findings {
        println!("{finding}");
    }
    if findings.is_empty() {
        println!(
            "mkmap: check: ok K1..K7 ({} members, {} shell, {} public, {} private)",
            summary.members, summary.shell_members, summary.public, summary.private
        );
        Ok(0)
    } else {
        eprintln!("mkmap: check: {} finding(s)", findings.len());
        Ok(1)
    }
}

fn run_freeze(args: &[String]) -> Result<u8> {
    let (flags, positionals) = parse_flags(args)?;
    if !positionals.is_empty() {
        bail!("freeze takes no positional argument");
    }
    let so = PathBuf::from(one(&flags, "so")?);
    let node = one(&flags, "node")?;
    let out = PathBuf::from(one(&flags, "out")?);
    let arch = flags
        .get("arch")
        .and_then(|values| values.first())
        .map(String::as_str)
        .unwrap_or("all");
    let root = std::env::current_dir().context("current directory")?;
    let rendered = raven_libc_mkmap::freeze::run(&so, node, &out, arch, &root)?;
    let count = rendered
        .lines()
        .filter(|line| !line.starts_with('#') && !line.trim().is_empty())
        .count();
    println!("mkmap: freeze: ok ({count} symbols in {node})");
    Ok(0)
}
