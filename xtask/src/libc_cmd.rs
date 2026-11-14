// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Dispatcher for the `libc-*` xtask commands.

use std::path::Path;

use anyhow::{Result, bail};

use crate::{
    libc_census, libc_headers, libc_link, libc_lint, libc_oracle, libc_pack, libc_size, libc_stubs,
    libc_world,
};

/// One command implementation.
pub(crate) type Runner = fn(&Path, &[String]) -> Result<u8>;

/// The command table.
pub(crate) const COMMANDS: &[(&str, Runner)] = &[
    ("libc-lint", run_lint),
    ("libc-oracle", libc_oracle::run),
    ("libc-headers", libc_headers::run),
    ("libc-link", libc_link::run),
    ("libc-pack", libc_pack::run),
    ("libc-size", libc_size::run),
    ("libc-world", libc_world::run),
    ("libc-census", libc_census::run),
    ("libc-stubs", libc_stubs::run),
];

fn run_lint(root: &Path, args: &[String]) -> Result<u8> {
    let mut checks = Vec::new();
    let mut json = false;
    let mut quiet = false;
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--json" => json = true,
            "--quiet" => quiet = true,
            "--root" => {
                index += 1;
                let value = args
                    .get(index)
                    .ok_or_else(|| anyhow::anyhow!("--root needs a value"))?;
                return run_lint_at(Path::new(value), &checks, json, quiet);
            }
            value if value.starts_with('-') => bail!("unknown libc-lint flag `{value}`"),
            value => checks.push(value.to_string()),
        }
        index += 1;
    }
    run_lint_at(root, &checks, json, quiet)
}

fn run_lint_at(root: &Path, checks: &[String], json: bool, quiet: bool) -> Result<u8> {
    let violations = libc_lint::run_checks(root, checks)?;
    let files = libc_lint::count_files(root);
    if json {
        println!("{}", libc_lint::json(&violations, files));
    } else if !quiet {
        print!("{}", libc_lint::render(&violations));
    }
    if violations.is_empty() {
        eprintln!("libc-lint: ok ({files} files scanned)");
        Ok(0)
    } else {
        eprintln!("libc-lint: {} violation(s)", violations.len());
        Ok(1)
    }
}

/// Whether `name` is a libc command.
#[must_use]
pub(crate) fn is_libc_command(name: &str) -> bool {
    COMMANDS.iter().any(|(candidate, _)| *candidate == name)
}

/// Run one libc command.
pub(crate) fn run(root: &Path, name: &str, args: &[String]) -> Result<u8> {
    let Some((_, runner)) = COMMANDS.iter().find(|(candidate, _)| *candidate == name) else {
        bail!("unknown libc command `{name}`");
    };
    runner(root, args)
}

#[cfg(test)]
#[path = "libc_cmd_ut.rs"]
mod tests;
