// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `raven-fuzz`: the deterministic fuzz driver for the S20 targets.
//!
//! ```text
//! raven-fuzz [--target <name>]... [--all] [--iterations <n>] [--seconds <s>]
//!            [--seed <n>] [--corpus <dir>] [--print-seed <name>]
//! ```
//!
//! Every run starts from the target's built-in seed and mutates it with a seeded xorshift PRNG,
//! so a crash is reproducible with the printed seed. The corpus directory, when given, is replayed
//! before the sweep: every `<dir>/<target>/*.hex` file is decoded and run once.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::{Duration, Instant};

use raven_fuzz_model::{Target, from_hex, fuzz, run as run_input, seed, to_hex};

const USAGE: &str = "\
usage:
  raven-fuzz --target <name> [--target <name>]... [options]
  raven-fuzz --all [options]
  raven-fuzz --print-seed <name>

targets: syscall usercopy handles elf ipc
options:
  --iterations <n>   inputs per target (default 10000)
  --seconds <s>      run each target for at least this many seconds
  --seed <n>         PRNG seed (default 1)
  --corpus <dir>     replay <dir>/<target>/*.hex first
  --print-seed <n>   print the target's seed as hex and exit
  --print-corpus     print the corpus format and exit";

fn main() -> ExitCode {
    match run() {
        Ok(code) => ExitCode::from(code),
        Err(message) => {
            eprintln!("raven-fuzz: {message}");
            ExitCode::from(2)
        }
    }
}

fn run() -> Result<u8, String> {
    let mut targets: Vec<Target> = Vec::new();
    let mut iterations = 10_000u64;
    let mut seconds = 0u64;
    let mut seed_value = 1u64;
    let mut corpus: Option<PathBuf> = None;
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--target" => {
                index += 1;
                targets.push(target(value(&args, index, "--target")?)?);
            }
            "--all" => targets.extend(Target::ALL),
            "--iterations" => {
                index += 1;
                iterations = value(&args, index, "--iterations")?
                    .parse()
                    .map_err(|_| "--iterations needs a number".to_string())?;
            }
            "--seconds" => {
                index += 1;
                seconds = value(&args, index, "--seconds")?
                    .parse()
                    .map_err(|_| "--seconds needs a number".to_string())?;
            }
            "--seed" => {
                index += 1;
                seed_value = value(&args, index, "--seed")?
                    .parse()
                    .map_err(|_| "--seed needs a number".to_string())?;
            }
            "--corpus" => {
                index += 1;
                corpus = Some(PathBuf::from(value(&args, index, "--corpus")?));
            }
            "--print-seed" => {
                index += 1;
                let target = target(value(&args, index, "--print-seed")?)?;
                print!("{}", to_hex(&seed(target)));
                return Ok(0);
            }
            "--print-corpus" => {
                println!(
                    "corpus: <dir>/<target>/*.hex, hex digit pairs, whitespace and # comments ignored"
                );
                return Ok(0);
            }
            "-h" | "--help" => {
                println!("{USAGE}");
                return Ok(0);
            }
            other => return Err(format!("unexpected argument `{other}`\n{USAGE}")),
        }
        index += 1;
    }
    if targets.is_empty() {
        return Err(format!("no target selected\n{USAGE}"));
    }
    targets.sort_by_key(|target| target.name());
    targets.dedup();

    println!(
        "raven-fuzz: seed {seed_value}, {} iteration(s), {} second(s) per target",
        iterations, seconds
    );
    let mut rows: BTreeMap<&str, (u64, u64, f64)> = BTreeMap::new();
    for target in targets {
        let started = Instant::now();
        let mut corpus_runs = 0u64;
        if let Some(directory) = &corpus {
            corpus_runs = replay_corpus(target, directory)?;
        }
        let mut executed = 0u64;
        let deadline = started + Duration::from_secs(seconds);
        loop {
            executed += fuzz(target, seed_value.wrapping_add(executed), iterations);
            if seconds == 0 || Instant::now() >= deadline {
                break;
            }
        }
        let elapsed = started.elapsed().as_secs_f64();
        println!(
            "  {}: {} inputs (+{} corpus), {:.2}s",
            target.name(),
            executed + corpus_runs,
            corpus_runs,
            elapsed
        );
        rows.insert(
            target.name(),
            (executed + corpus_runs, corpus_runs, elapsed),
        );
    }
    let total: u64 = rows.values().map(|row| row.0).sum();
    println!("raven-fuzz: {total} inputs, no invariant violation");
    Ok(0)
}

/// Replay `<directory>/<target>/*.hex`; returns the number of inputs run.
fn replay_corpus(target: Target, directory: &Path) -> Result<u64, String> {
    let folder = directory.join(target.name());
    let entries = match fs::read_dir(&folder) {
        Ok(entries) => entries,
        Err(_) => return Ok(0),
    };
    let mut count = 0;
    let mut paths: Vec<PathBuf> = entries
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .filter(|path| path.extension().and_then(|value| value.to_str()) == Some("hex"))
        .collect();
    paths.sort();
    for path in paths {
        let text = fs::read_to_string(&path)
            .map_err(|error| format!("cannot read {}: {error}", path.display()))?;
        let bytes =
            from_hex(&text).map_err(|error| format!("{}: bad hex: {error:?}", path.display()))?;
        run_input(target, &bytes);
        count += 1;
    }
    Ok(count)
}

fn target(name: &str) -> Result<Target, String> {
    Target::from_name(name).ok_or_else(|| format!("unknown target `{name}`\n{USAGE}"))
}

fn value<'a>(args: &'a [String], index: usize, flag: &str) -> Result<&'a str, String> {
    args.get(index)
        .map(String::as_str)
        .ok_or_else(|| format!("{flag} needs a value"))
}
