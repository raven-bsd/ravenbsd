// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Create the small raw disk the S205 virtio-blk reload case attaches.
//!
//! The driver's test mode formats its own partition and data patterns, so the fixture only needs
//! a writable device of the expected capacity. Keeping the disk at 2 MiB makes the pattern pass
//! short enough that the completion bound is not exercised by the host's I/O latency.

use std::env;
use std::fs::OpenOptions;
use std::process::ExitCode;

const SECTOR: u64 = 512;
const SECTORS: u64 = 4096;

fn main() -> ExitCode {
    let path = match env::args().nth(1) {
        Some(path) => path,
        None => {
            eprintln!("usage: s205-virtio-blk-mkdisk <output>");
            return ExitCode::FAILURE;
        }
    };
    let file = OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .open(&path);
    let file = match file {
        Ok(file) => file,
        Err(error) => {
            eprintln!("cannot create {path}: {error}");
            return ExitCode::FAILURE;
        }
    };
    if let Err(error) = file.set_len(SECTORS * SECTOR) {
        eprintln!("cannot size {path}: {error}");
        return ExitCode::FAILURE;
    }
    println!("{path}: {} bytes", SECTORS * SECTOR);
    ExitCode::SUCCESS
}
