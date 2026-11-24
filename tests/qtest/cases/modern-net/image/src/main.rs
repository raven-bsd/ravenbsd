// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Copy an initfs image and add extra files.
//!
//! Usage: `modern-net-image <input.img> <output.img> <path-in-image>=<host-file>...`

use std::env;
use std::fs;
use std::process::ExitCode;

use raven_initfs::{Initfs, InitfsBuilder};

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("modern-net-image: {error}");
            ExitCode::FAILURE
        }
    }
}

fn usage() -> String {
    String::from("usage: modern-net-image <input.img> <output.img> <path>=<host-file>...")
}

fn run() -> Result<(), String> {
    let mut args = env::args().skip(1);
    let input = args.next().ok_or_else(usage)?;
    let output = args.next().ok_or_else(usage)?;
    let specs: Vec<String> = args.collect();
    let image = fs::read(&input).map_err(|error| format!("{input}: {error}"))?;
    let initfs = Initfs::parse(&image).map_err(|error| format!("{input}: {error}"))?;
    let mut replacements = Vec::new();
    for spec in &specs {
        let (path, host) = spec
            .split_once('=')
            .ok_or_else(|| format!("bad file spec '{spec}': expected <path>=<host-file>"))?;
        replacements.push((path.to_string(), host.to_string()));
    }
    let mut builder = InitfsBuilder::new();
    for entry in initfs.entries() {
        if replacements.iter().any(|(path, _)| path == entry.path) {
            continue;
        }
        builder
            .add_file(entry.path, entry.data, entry.mode)
            .map_err(|error| format!("{}: {error}", entry.path))?;
    }
    for (path, host) in &replacements {
        let data = fs::read(host).map_err(|error| format!("{host}: {error}"))?;
        builder
            .add_file(path, &data, 0o755)
            .map_err(|error| format!("{path}: {error}"))?;
    }
    fs::write(&output, builder.build()).map_err(|error| format!("{output}: {error}"))
}
