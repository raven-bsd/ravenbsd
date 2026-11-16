// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Deterministic corpus generator and stable mutational smoke runner.

use std::collections::BTreeMap;
use std::env;
use std::fs;
use std::path::PathBuf;
use std::process::ExitCode;
use std::time::{Duration, Instant};

use raven_fs_fuzz_replay::{TARGETS, crash_matrix, region, replay};

fn main() -> ExitCode {
    let args: Vec<String> = env::args().skip(1).collect();
    let result = match args.first().map(String::as_str) {
        Some("seed") => seed(&args[1..]),
        Some("run") => run(&args[1..]),
        Some("crash") => crash(),
        _ => {
            eprintln!(
                "usage: raven-fs-fuzz seed [--out <dir>] | run [--seconds <n>] [--seed <n>] [--target <name>] | crash"
            );
            return ExitCode::from(2);
        }
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("raven-fs-fuzz: {error}");
            ExitCode::from(1)
        }
    }
}

fn seed(args: &[String]) -> Result<(), String> {
    let mut out = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fs-fuzz/corpus");
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--out" => {
                index += 1;
                out = PathBuf::from(args.get(index).ok_or("--out needs a path")?);
            }
            other => return Err(format!("unexpected seed argument {other}")),
        }
        index += 1;
    }
    for target in TARGETS {
        let region = region(target)?;
        let directory = out.join(target);
        fs::create_dir_all(&directory).map_err(|error| error.to_string())?;
        let pristine = region
            .base
            .get(region.offset..region.offset + region.length)
            .ok_or("region slice")?
            .to_vec();
        write_seed(&directory.join("seed-00"), &pristine)?;
        // The whole-image GPT/FAT parsers get a wider corpus than the UFS overlays: the ticket
        // requires at least twenty malformed vectors next to the eight-seed baseline.
        let count = if *target == "gpt" || *target == "fat" {
            24usize
        } else {
            8usize
        };
        for seed in 1..count {
            let mut data = pristine.clone();
            let mut state = 0x9e37_79b9_7f4a_7c15u64 ^ seed as u64;
            for _ in 0..(2 + seed % 3) {
                state ^= state << 13;
                state ^= state >> 7;
                state ^= state << 17;
                let at = (state as usize) % data.len();
                data[at] ^= (state >> 32) as u8 | 1;
            }
            write_seed(&directory.join(format!("seed-{seed:02}")), &data)?;
        }
        println!("seed {target}: {count} files in {}", directory.display());
    }
    Ok(())
}

fn crash() -> Result<(), String> {
    let report = crash_matrix()?;
    let mut by_stage: BTreeMap<&str, (u64, u64, u64, u64)> = BTreeMap::new();
    for cut in &report.cuts {
        let entry = by_stage.entry(cut.stage.as_str()).or_default();
        entry.0 += 1;
        if cut.check == "clean" || cut.check == "repaired" {
            entry.1 += 1;
        } else {
            entry.3 += 1;
        }
    }
    for (stage, counts) in &by_stage {
        println!(
            "{stage}\tcuts={}\trecovered={}\tunresolved={}",
            counts.0, counts.1, counts.3
        );
    }
    println!(
        "total\twrites={}\tcuts={}",
        report.total_writes,
        report.cuts.len()
    );
    Ok(())
}

fn write_seed(path: &PathBuf, data: &[u8]) -> Result<(), String> {
    fs::write(path, data).map_err(|error| format!("write {}: {error}", path.display()))
}

fn run(args: &[String]) -> Result<(), String> {
    let mut seconds = 10u64;
    let mut state = 0x1234_5678_9abc_def0u64;
    let mut only_target: Option<&str> = None;
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--seconds" => {
                index += 1;
                seconds = args
                    .get(index)
                    .ok_or("--seconds needs a value")?
                    .parse()
                    .map_err(|_| "--seconds is not a number")?;
            }
            "--seed" => {
                index += 1;
                state = args
                    .get(index)
                    .ok_or("--seed needs a value")?
                    .parse()
                    .map_err(|_| "--seed is not a number")?;
            }
            "--target" => {
                index += 1;
                only_target = Some(args.get(index).ok_or("--target needs a value")?);
            }
            other => return Err(format!("unexpected run argument {other}")),
        }
        index += 1;
    }
    for target in TARGETS {
        if only_target.is_some_and(|name| name != *target) {
            continue;
        }
        let deadline = Instant::now() + Duration::from_secs(seconds);
        let region = region(target)?;
        let mut executions = 0u64;
        while Instant::now() < deadline {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            let mut data = region
                .base
                .get(region.offset..region.offset + region.length)
                .ok_or("region slice")?
                .to_vec();
            let flips = 1 + (state as usize % 8);
            for _ in 0..flips {
                state ^= state << 13;
                state ^= state >> 7;
                state ^= state << 17;
                let at = (state as usize) % data.len();
                data[at] ^= ((state >> 32) as u8) | 1;
            }
            replay(target, &data)?;
            executions += 1;
        }
        println!("{target}: {executions} executions in {seconds}s (seed {state:016x})");
    }
    Ok(())
}
