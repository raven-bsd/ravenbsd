// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! List the contents of an initfs image.
//!
//! Usage: `lsinitfs <image>`; prints one line per file: path, size, mode, CRC-32.

#![forbid(unsafe_code)]

use std::env;
use std::fs;
use std::process::ExitCode;

use raven_initfs::{Initfs, crc32};

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("lsinitfs: {error}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<(), String> {
    let mut args = env::args_os().skip(1);
    let path = args.next().ok_or_else(usage)?;
    if args.next().is_some() {
        return Err(usage());
    }

    let image = fs::read(&path).map_err(|error| format!("{}: {error}", path.to_string_lossy()))?;
    let initfs =
        Initfs::parse(&image).map_err(|error| format!("{}: {error}", path.to_string_lossy()))?;

    for entry in initfs.entries() {
        println!(
            "{} {} {:04o} {:08x}",
            entry.path,
            entry.data.len(),
            entry.mode,
            crc32(entry.data)
        );
    }
    Ok(())
}

fn usage() -> String {
    String::from("usage: lsinitfs <image>")
}
