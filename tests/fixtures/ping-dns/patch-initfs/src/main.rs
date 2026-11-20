// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Copy a release initfs, add `/bin/ping-dns-dnsd`, and stage a test-only `/etc/resolv.conf`
//! with `nameserver 127.0.0.1` (the loopback DNS fixture).
//!
//! Usage: `ping-dns-patch <release-initfs> <ping-dns-dnsd> <output-initfs>`.
//! The release entries keep their paths, bytes and modes; only the fixture and the test resolver
//! line are added.

#![forbid(unsafe_code)]

use std::env;
use std::fs;
use std::process::ExitCode;

use raven_initfs::{Initfs, InitfsBuilder};

const RESOLV_CONF: &str = "/etc/resolv.conf";
const RESOLV_LINE: &[u8] = b"nameserver 127.0.0.1\n";
const FIXTURE: &str = "/bin/ping-dns-dnsd";

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("ping-dns-patch: {error}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<(), String> {
    let mut args = env::args().skip(1);
    let input = args.next().ok_or_else(usage)?;
    let fixture = args.next().ok_or_else(usage)?;
    let output = args.next().ok_or_else(usage)?;
    if args.next().is_some() {
        return Err(usage());
    }

    let image = fs::read(&input).map_err(|error| format!("{input}: {error}"))?;
    let initfs = Initfs::parse(&image).map_err(|error| format!("{input}: {error}"))?;
    let fixture_data = fs::read(&fixture).map_err(|error| format!("{fixture}: {error}"))?;

    let mut builder = InitfsBuilder::new();
    for entry in initfs.entries() {
        if entry.path == RESOLV_CONF {
            continue;
        }
        builder
            .add_file(entry.path, entry.data, entry.mode)
            .map_err(|error| format!("{}: {error}", entry.path))?;
    }
    builder
        .add_file(FIXTURE, &fixture_data, 0o755)
        .map_err(|error| format!("{FIXTURE}: {error}"))?;
    builder
        .add_file(RESOLV_CONF, RESOLV_LINE, 0o644)
        .map_err(|error| format!("{RESOLV_CONF}: {error}"))?;

    fs::write(&output, builder.build()).map_err(|error| format!("{output}: {error}"))?;
    println!("ping-dns-patch: {output} {FIXTURE} {RESOLV_CONF}");
    Ok(())
}

fn usage() -> String {
    String::from("usage: ping-dns-patch <release-initfs> <ping-dns-dnsd> <output-initfs>")
}
