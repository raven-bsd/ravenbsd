// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Build an initfs image from host files.
//!
//! Usage: `mkinitfs <out.img> <path-in-image>=<host-file>[:<octal-mode>]...`
//! The default mode is `0755` under `/init`, `/bin/` and `/sbin/`, and `0644` elsewhere.

#![forbid(unsafe_code)]

use std::env;
use std::fs;
use std::path::PathBuf;
use std::process::ExitCode;

use raven_initfs::InitfsBuilder;

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("mkinitfs: {error}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<(), String> {
    let mut args = env::args().skip(1);
    let output = args.next().ok_or_else(usage)?;
    let specs: Vec<String> = args.collect();
    if specs.is_empty() {
        return Err(usage());
    }

    let mut builder = InitfsBuilder::new();
    for spec in &specs {
        let (path, host, mode) = parse_spec(spec)?;
        let data = fs::read(&host).map_err(|error| format!("{}: {error}", host.display()))?;
        builder
            .add_file(&path, &data, mode)
            .map_err(|error| format!("{path}: {error}"))?;
    }

    fs::write(&output, builder.build()).map_err(|error| format!("{output}: {error}"))
}

fn usage() -> String {
    String::from("usage: mkinitfs <out.img> <path-in-image>=<host-file>[:<octal-mode>]...")
}

fn parse_spec(spec: &str) -> Result<(String, PathBuf, u32), String> {
    let (path, rest) = spec.split_once('=').ok_or_else(|| {
        format!("bad file spec '{spec}': expected <path-in-image>=<host-file>[:<mode>]")
    })?;
    let (host, mode) = split_mode(rest);
    let mode = mode.unwrap_or_else(|| default_mode(path));
    Ok((String::from(path), PathBuf::from(host), mode))
}

/// Split an optional trailing `:<octal-mode>`; anything that is not a valid mode is part of the
/// host file name.
fn split_mode(rest: &str) -> (&str, Option<u32>) {
    if let Some((host, mode_text)) = rest.rsplit_once(':')
        && !mode_text.is_empty()
        && mode_text.chars().all(|character| character.is_digit(8))
        && let Ok(mode) = u32::from_str_radix(mode_text, 8)
        && mode <= 0o7777
    {
        return (host, Some(mode));
    }
    (rest, None)
}

fn default_mode(path: &str) -> u32 {
    if path == "/init" || path.starts_with("/bin/") || path.starts_with("/sbin/") {
        0o755
    } else {
        0o644
    }
}
