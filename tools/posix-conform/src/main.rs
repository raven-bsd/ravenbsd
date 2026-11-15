// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `posix-conform` command line: seed, generate and check.

use std::path::PathBuf;

use anyhow::{Context, Result, bail};

use posix_conform::model::{load_profile, load_trace, validate};
use posix_conform::render::{check_current, generate};
use posix_conform::seed;

fn main() -> std::process::ExitCode {
    match run(std::env::args().skip(1).collect()) {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("posix-conform: {error:#}");
            std::process::ExitCode::from(1)
        }
    }
}

fn run(args: Vec<String>) -> Result<()> {
    let mut command = None;
    let mut root = PathBuf::from(".");
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--root" => {
                index += 1;
                root = PathBuf::from(
                    args.get(index)
                        .ok_or_else(|| anyhow::anyhow!("--root needs a value"))?,
                );
            }
            value if value.starts_with('-') => bail!("unknown option `{value}`"),
            value => {
                if command.is_some() {
                    bail!("unexpected argument `{value}`");
                }
                command = Some(value.to_string());
            }
        }
        index += 1;
    }
    let command = command.ok_or_else(|| anyhow::anyhow!("expected seed, generate or check"))?;
    match command.as_str() {
        "seed" => seed::seed(&root),
        "generate" => {
            let profile = load_profile(&root.join("docs/posix/profile.toml"))
                .context("cannot load profile")?;
            let trace = load_trace(&root.join("docs/posix/traceability.toml"))
                .context("cannot load traceability")?;
            validate(&root, &profile, &trace)?;
            generate(&root, &profile, &trace)
        }
        "check" => {
            let profile = load_profile(&root.join("docs/posix/profile.toml"))
                .context("cannot load profile")?;
            let trace = load_trace(&root.join("docs/posix/traceability.toml"))
                .context("cannot load traceability")?;
            validate(&root, &profile, &trace)?;
            check_current(&root, &profile, &trace)?;
            let ids = profile.unique_ids()?.len();
            println!("posix-gaps: {ids} requirements validated");
            Ok(())
        }
        other => bail!("unknown command `{other}` (expected seed, generate or check)"),
    }
}
