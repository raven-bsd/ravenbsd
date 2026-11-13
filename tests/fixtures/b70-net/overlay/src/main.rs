// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Rebuild an initfs image with test-only file replacements.
//!
//! The B70 profile extends the production `server` image, so its `/etc/svcd.conf`
//! cannot be overridden by another profile entry. This small fixture tool copies
//! every entry of the packed initfs and replaces/adds only the paths named on
//! the command line. It never changes the profile or any product binary.
//!
//! Usage: `raven-b70-overlay <source.img> <output.img> <path=host-file[:mode]>...`

#![forbid(unsafe_code)]

use std::collections::BTreeMap;
use std::env;
use std::fs;
use std::path::PathBuf;
use std::process::ExitCode;

use raven_initfs::{Initfs, InitfsBuilder};

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("b70-overlay: {error}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<(), String> {
    let mut args = env::args().skip(1);
    let source = PathBuf::from(args.next().ok_or_else(usage)?);
    let output = PathBuf::from(args.next().ok_or_else(usage)?);
    let specs: Vec<String> = args.collect();
    if specs.is_empty() {
        return Err(usage());
    }

    let mut replacements = BTreeMap::new();
    for spec in &specs {
        let (path, host, mode) = parse_spec(spec)?;
        if replacements.insert(path.clone(), (host, mode)).is_some() {
            return Err(format!("duplicate replacement path {path}"));
        }
    }

    let image_bytes =
        fs::read(&source).map_err(|error| format!("{}: {error}", source.display()))?;
    let image =
        Initfs::parse(&image_bytes).map_err(|error| format!("{}: {error}", source.display()))?;
    let mut builder = InitfsBuilder::new();
    for entry in image.entries() {
        if replacements.contains_key(entry.path) {
            continue;
        }
        builder
            .add_file(entry.path, entry.data, entry.mode)
            .map_err(|error| format!("{}: {error}", entry.path))?;
    }
    for (path, (host, mode)) in replacements {
        let data = fs::read(&host).map_err(|error| format!("{}: {error}", host.display()))?;
        builder
            .add_file(&path, &data, mode)
            .map_err(|error| format!("{path}: {error}"))?;
    }
    fs::write(&output, builder.build()).map_err(|error| format!("{}: {error}", output.display()))
}

fn usage() -> String {
    String::from("usage: raven-b70-overlay <source.img> <output.img> <path=host-file[:mode]>...")
}

fn parse_spec(spec: &str) -> Result<(String, PathBuf, u32), String> {
    let (path, rest) = spec.split_once('=').ok_or_else(|| {
        format!("bad file spec '{spec}': expected <path-in-image>=<host-file>[:<mode>]")
    })?;
    let (host, mode) = split_mode(rest);
    let mode = mode.unwrap_or_else(|| default_mode(path));
    Ok((String::from(path), PathBuf::from(host), mode))
}

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
