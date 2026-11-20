// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Copy a release initfs, add `/bin/ping-dns-dnsd`, and stage a test-only `/etc/resolv.conf`
//! with `nameserver 127.0.0.1` (the loopback DNS fixture).
//!
//! Usage: `ping-dns-patch <release-initfs> <ping-dns-dnsd> <output-initfs>`.
//! The release entries keep their paths, bytes and modes except for two test-only changes: the
//! fixture and test resolver line, and a `scheme:resolv` need on the getty service. The stock
//! release starts getty before resolvd, so the interactive shell's scheme-namespace snapshot
//! lacks `resolv:`; the test schedules getty after resolvd to exercise ping's resolver path.
//! Changing the production service order is outside this ticket's Owns paths.

#![forbid(unsafe_code)]

use std::env;
use std::fs;
use std::process::ExitCode;

use raven_initfs::{Initfs, InitfsBuilder};

const RESOLV_CONF: &str = "/etc/resolv.conf";
const RESOLV_LINE: &[u8] = b"nameserver 127.0.0.1\n";
const FIXTURE: &str = "/bin/ping-dns-dnsd";
const GETTY_MANIFEST: &str = "/etc/services.d/getty.toml";
const GETTY_NEEDS: &[u8] = b"\"scheme:resolv\", ";

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
    let mut getty_patched = false;
    for entry in initfs.entries() {
        if entry.path == RESOLV_CONF {
            continue;
        }
        let data = if entry.path == GETTY_MANIFEST {
            getty_patched = true;
            add_resolv_need(entry.data)?
        } else {
            entry.data.to_vec()
        };
        builder
            .add_file(entry.path, &data, entry.mode)
            .map_err(|error| format!("{}: {error}", entry.path))?;
    }
    if !getty_patched {
        return Err(format!("{GETTY_MANIFEST} is not in the release initfs"));
    }
    builder
        .add_file(FIXTURE, &fixture_data, 0o755)
        .map_err(|error| format!("{FIXTURE}: {error}"))?;
    builder
        .add_file(RESOLV_CONF, RESOLV_LINE, 0o644)
        .map_err(|error| format!("{RESOLV_CONF}: {error}"))?;

    fs::write(&output, builder.build()).map_err(|error| format!("{output}: {error}"))?;
    println!("ping-dns-patch: {output} {FIXTURE} {RESOLV_CONF} {GETTY_MANIFEST}");
    Ok(())
}

/// Add `scheme:resolv` to the getty service's `needs` list.
///
/// A child's scheme namespace is snapshotted when the process is created. The stock release
/// creates getty before resolvd registers `resolv:`, so the login shell and its children cannot
/// open the resolver. The test needs getty (and therefore the shell and ping) to start after
/// resolvd; this edit is test-only and does not touch production resolver policy.
fn add_resolv_need(data: &[u8]) -> Result<Vec<u8>, String> {
    let text = core::str::from_utf8(data).map_err(|_| "getty.toml is not UTF-8".to_string())?;
    if text.contains("\"scheme:resolv\"") {
        return Ok(data.to_vec());
    }
    let marker = "needs = [";
    let index = text
        .find(marker)
        .ok_or_else(|| "getty.toml has no needs list".to_string())?;
    let insert = index + marker.len();
    let mut out = Vec::with_capacity(data.len() + GETTY_NEEDS.len());
    out.extend_from_slice(&data[..insert]);
    out.extend_from_slice(GETTY_NEEDS);
    out.extend_from_slice(&data[insert..]);
    Ok(out)
}

fn usage() -> String {
    String::from("usage: ping-dns-patch <release-initfs> <ping-dns-dnsd> <output-initfs>")
}
