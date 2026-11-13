// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `libc-oracle` command: build donor C originals and run the differential harness.

use std::path::Path;

use anyhow::{Result, bail};

use crate::process::{CommandSpec, run as run_command};

const USAGE: &str = "\
usage:
  cargo xtask libc-oracle <donor> <file>
  cargo xtask libc-oracle build <family> [flags]
  cargo xtask libc-oracle list
  cargo xtask libc-oracle check
  cargo xtask libc-oracle vendor <flags>
  cargo xtask libc-oracle diff <family> [--seeds N] [--exhaustive] [--opt O0,O2]
                                    [--sanitize] [--no-pin-check]";

/// One planned process and the line printed when it finishes.
struct Planned {
    spec: CommandSpec,
    label: Option<String>,
}

/// Run the command.
pub(crate) fn run(root: &Path, args: &[String]) -> Result<u8> {
    let Some(first) = args.first() else {
        bail!("libc-oracle needs a subcommand\n{USAGE}");
    };
    if first == "--help" || first == "-h" || first == "help" {
        println!("{USAGE}");
        return Ok(0);
    }
    let planned = plan(root, args)?;
    let mut failed = false;
    for step in planned {
        let code = run_command(&step.spec)?;
        if let Some(label) = &step.label {
            if code == 0 {
                println!("{label}: ok");
            } else if code == 77 {
                println!("{label}: skipped");
            } else {
                println!("{label}: FAILED");
            }
        }
        if code == 77 {
            return Ok(77);
        }
        if code != 0 {
            failed = true;
        }
    }
    Ok(u8::from(failed))
}

fn plan(root: &Path, args: &[String]) -> Result<Vec<Planned>> {
    match classify(args)? {
        Kind::Forward(forward) => Ok(vec![forward_step(root, &forward, Vec::new())]),
        Kind::Adhoc { donor, file } => Ok(vec![forward_step(
            root,
            &["adhoc".to_string(), donor, file],
            Vec::new(),
        )]),
        Kind::Diff(diff) => diff.steps(root),
    }
}

enum Kind {
    Forward(Vec<String>),
    Adhoc { donor: String, file: String },
    Diff(Diff),
}

struct Diff {
    family: String,
    opts: Vec<String>,
    seeds: Option<u64>,
    exhaustive: bool,
    sanitize: bool,
    no_pin_check: bool,
}

impl Diff {
    fn steps(&self, root: &Path) -> Result<Vec<Planned>> {
        let mut build = vec![
            "build".to_string(),
            self.family.clone(),
            "--opt".to_string(),
            self.opts.join(","),
        ];
        if self.sanitize {
            build.push("--sanitize".to_string());
        }
        if self.no_pin_check {
            build.push("--no-pin-check".to_string());
        }
        let mut planned = vec![forward_step(root, &build, Vec::new())];
        for opt in &self.opts {
            let directory = root.join("build/libc-oracle").join(&self.family).join(opt);
            let directory = directory.display().to_string();
            let mut env = vec![
                ("LIBRARY_PATH".to_string(), directory.clone()),
                ("LD_LIBRARY_PATH".to_string(), directory.clone()),
                ("RAVEN_ORACLE_DIR".to_string(), directory),
            ];
            if let Some(seeds) = self.seeds {
                env.push(("RL_DIFF_SEEDS".to_string(), seeds.to_string()));
            }
            if self.exhaustive {
                env.push(("RL_DIFF_EXHAUSTIVE".to_string(), "1".to_string()));
            }
            let mut spec = CommandSpec::new(
                "cargo",
                vec![
                    "test".to_string(),
                    "-p".to_string(),
                    "raven-libc-oracle".to_string(),
                    "--features".to_string(),
                    format!("oracle-{}", self.family),
                ],
                root,
            );
            for (key, value) in env {
                spec = spec.env(key, value);
            }
            planned.push(Planned {
                spec,
                label: Some(format!("libc-oracle diff {} {opt}", self.family)),
            });
        }
        Ok(planned)
    }
}

fn classify(args: &[String]) -> Result<Kind> {
    let Some(first) = args.first() else {
        bail!("libc-oracle needs a subcommand\n{USAGE}");
    };
    match first.as_str() {
        "build" | "list" | "check" | "vendor" => Ok(Kind::Forward(args.to_vec())),
        "diff" => classify_diff(&args[1..]),
        flag if flag.starts_with('-') => bail!("unknown libc-oracle flag `{flag}`\n{USAGE}"),
        donor => {
            if args.len() != 2 || args[1].starts_with('-') {
                bail!("libc-oracle needs exactly <donor> <file>\n{USAGE}");
            }
            Ok(Kind::Adhoc {
                donor: donor.to_string(),
                file: args[1].clone(),
            })
        }
    }
}

fn classify_diff(args: &[String]) -> Result<Kind> {
    let mut family = None;
    let mut opts = vec!["O2".to_string(), "O0".to_string()];
    let mut seeds = None;
    let mut exhaustive = false;
    let mut sanitize = false;
    let mut no_pin_check = false;
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--seeds" => {
                index += 1;
                let value = args
                    .get(index)
                    .ok_or_else(|| anyhow::anyhow!("--seeds needs a value"))?;
                seeds = Some(
                    value
                        .parse::<u64>()
                        .map_err(|_| anyhow::anyhow!("--seeds `{value}` is not an integer"))?,
                );
            }
            "--exhaustive" => exhaustive = true,
            "--sanitize" => sanitize = true,
            "--no-pin-check" => no_pin_check = true,
            "--opt" => {
                index += 1;
                let value = args
                    .get(index)
                    .ok_or_else(|| anyhow::anyhow!("--opt needs a value"))?;
                opts = parse_opts(value)?;
            }
            flag if flag.starts_with('-') => bail!("unknown diff flag `{flag}`"),
            value => {
                if family.replace(value.to_string()).is_some() {
                    bail!("diff takes one family");
                }
            }
        }
        index += 1;
    }
    let Some(family) = family else {
        bail!("diff needs a family\n{USAGE}");
    };
    Ok(Kind::Diff(Diff {
        family,
        opts,
        seeds,
        exhaustive,
        sanitize,
        no_pin_check,
    }))
}

fn parse_opts(value: &str) -> Result<Vec<String>> {
    let mut levels = Vec::new();
    for level in value.split(',') {
        if !matches!(level, "O0" | "O1" | "O2" | "O3" | "Os") {
            bail!("unknown optimisation level `{level}`");
        }
        if levels.iter().any(|seen| seen == level) {
            bail!("optimisation level `{level}` repeated");
        }
        levels.push(level.to_string());
    }
    if levels.is_empty() {
        bail!("--opt needs at least one level");
    }
    Ok(levels)
}

fn forward_step(root: &Path, forwarded: &[String], env: Vec<(String, String)>) -> Planned {
    let mut args = vec!["run".to_string(), "-q".to_string(), "-p".to_string()];
    args.push("libc-oracle".to_string());
    args.push("--".to_string());
    args.extend(forwarded.iter().cloned());
    let mut spec = CommandSpec::new("cargo", args, root);
    for (key, value) in env {
        spec = spec.env(key, value);
    }
    Planned { spec, label: None }
}

#[cfg(test)]
#[path = "libc_oracle_ut.rs"]
mod tests;
