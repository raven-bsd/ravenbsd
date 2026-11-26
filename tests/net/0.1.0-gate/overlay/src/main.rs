// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Rebuild an initfs image with test-only file replacements.
//!
//! The release profile's `/etc/svcd.conf` and service directory are fixed by
//! the product profile. S85n adds one probe service and replaces that one file
//! without changing the profile, so this tool copies every entry of the packed
//! release initfs and replaces/adds only the paths named on the command line.
//!
//! Usage: `raven-net-gate-overlay <source.img> <output.img> <path=host-file[:0oMODE]>...`

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
            eprintln!("net-gate-overlay: {error}");
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
    String::from(
        "usage: raven-net-gate-overlay <source.img> <output.img> <path=host-file[:0oMODE]>...",
    )
}

fn parse_spec(spec: &str) -> Result<(String, PathBuf, u32), String> {
    let (path, rest) = spec.split_once('=').ok_or_else(|| {
        format!("bad file spec '{spec}': expected <path-in-image>=<host-file>[:0oMODE]")
    })?;
    if !path.starts_with('/') {
        return Err(format!(
            "bad image path '{path}': expected an absolute path"
        ));
    }
    let (host, mode) = split_mode(rest)?;
    Ok((path.to_string(), PathBuf::from(host), mode))
}

fn split_mode(rest: &str) -> Result<(&str, u32), String> {
    if let Some((host, mode)) = rest.rsplit_once(':')
        && let Some(digits) = mode.strip_prefix("0o")
    {
        let mode = u32::from_str_radix(digits, 8)
            .map_err(|_| format!("bad mode '{mode}': expected octal 0oMODE"))?;
        if host.is_empty() {
            return Err(String::from("empty host path in file spec"));
        }
        return Ok((host, mode));
    }
    if rest.is_empty() {
        return Err(String::from("empty host path in file spec"));
    }
    Ok((rest, 0o644))
}
