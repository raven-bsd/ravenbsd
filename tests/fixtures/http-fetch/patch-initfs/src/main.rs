// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Copy a release initfs and add one probe binary at `/bin/http-fetch`.
//!
//! Usage: `http-fetch-patch <release-initfs> <http-fetch-binary> <output-initfs>`.
//! The release entries keep their paths, bytes and modes; only the probe is added.

#![forbid(unsafe_code)]

use std::env;
use std::fs;
use std::process::ExitCode;

use raven_initfs::{Initfs, InitfsBuilder};

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("http-fetch-patch: {error}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<(), String> {
    let mut args = env::args().skip(1);
    let input = args.next().ok_or_else(usage)?;
    let probe = args.next().ok_or_else(usage)?;
    let output = args.next().ok_or_else(usage)?;
    if args.next().is_some() {
        return Err(usage());
    }

    let image = fs::read(&input).map_err(|error| format!("{input}: {error}"))?;
    let initfs = Initfs::parse(&image).map_err(|error| format!("{input}: {error}"))?;
    let probe_data = fs::read(&probe).map_err(|error| format!("{probe}: {error}"))?;

    let mut builder = InitfsBuilder::new();
    for entry in initfs.entries() {
        builder
            .add_file(entry.path, entry.data, entry.mode)
            .map_err(|error| format!("{}: {error}", entry.path))?;
    }
    builder
        .add_file("/bin/http-fetch", &probe_data, 0o755)
        .map_err(|error| format!("/bin/http-fetch: {error}"))?;

    fs::write(&output, builder.build()).map_err(|error| format!("{output}: {error}"))
}

fn usage() -> String {
    String::from("usage: http-fetch-patch <release-initfs> <http-fetch-binary> <output-initfs>")
}
