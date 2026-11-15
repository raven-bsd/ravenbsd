// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `posix-conform` command line: seed, generate, check and gate.

use std::path::PathBuf;

use anyhow::{Context, Result, bail};

use posix_conform::gate;
use posix_conform::model::{load_profile, load_trace, validate};
use posix_conform::render::{check_current, generate};
use posix_conform::seed;

fn main() -> std::process::ExitCode {
    match run(std::env::args().skip(1).collect()) {
        Ok(code) => std::process::ExitCode::from(code),
        Err(error) => {
            eprintln!("posix-conform: {error:#}");
            std::process::ExitCode::from(2)
        }
    }
}

fn run(args: Vec<String>) -> Result<u8> {
    let mut command = None;
    let mut root = PathBuf::from(".");
    let mut profile = "posix".to_string();
    let mut arch = "x86_64".to_string();
    let mut json = false;
    let mut host_only = false;
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
            "--profile" => {
                index += 1;
                profile = args
                    .get(index)
                    .ok_or_else(|| anyhow::anyhow!("--profile needs a value"))?
                    .clone();
            }
            "--arch" => {
                index += 1;
                arch = args
                    .get(index)
                    .ok_or_else(|| anyhow::anyhow!("--arch needs a value"))?
                    .clone();
            }
            "--json" => json = true,
            "--host-only" => host_only = true,
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
    let command =
        command.ok_or_else(|| anyhow::anyhow!("expected seed, generate, check or gate"))?;
    match command.as_str() {
        "seed" => {
            seed::seed(&root)?;
            Ok(0)
        }
        "generate" => {
            let profile = load_profile(&root.join("docs/posix/profile.toml"))
                .context("cannot load profile")?;
            let trace = load_trace(&root.join("docs/posix/traceability.toml"))
                .context("cannot load traceability")?;
            validate(&root, &profile, &trace)?;
            generate(&root, &profile, &trace)?;
            Ok(0)
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
            Ok(0)
        }
        "gate" => {
            let profile_doc = load_profile(&root.join("docs/posix/profile.toml"))
                .context("cannot load profile")?;
            let trace = load_trace(&root.join("docs/posix/traceability.toml"))
                .context("cannot load traceability")?;
            let options = gate::Options {
                profile,
                arch,
                json,
                host_only,
            };
            let report = gate::run(&root, &profile_doc, &trace, &options)?;
            let text = gate::render(&report);
            std::fs::write(root.join("docs/posix/CONFORMANCE.md"), text)
                .context("cannot write CONFORMANCE.md")?;
            if json {
                println!("{}", gate::json(&report)?);
            } else {
                println!(
                    "posix-check: {} pass, {} fail, {} skip; {}",
                    report.pass,
                    report.fail,
                    report.skip,
                    if report.release_blocked {
                        "FAIL (release blocked)"
                    } else {
                        "PASS"
                    }
                );
            }
            Ok(u8::from(report.release_blocked))
        }
        other => bail!("unknown command `{other}` (expected seed, generate, check or gate)"),
    }
}
