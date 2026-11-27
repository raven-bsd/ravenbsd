// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Add the target probe to the shell-test initfs.
//!
//! The qtest needs the full shell image (services, vfsd, initfsd) so `statfs(2)` has a mounted
//! root, plus one extra program. Rebuild the profile's initfs with the entry added, preserving
//! every entry and the image epoch.

use std::env;
use std::fs;
use std::process::ExitCode;

use raven_initfs::{Initfs, InitfsBuilder};

fn main() -> ExitCode {
    let args: Vec<String> = env::args().collect();
    if args.len() != 6 {
        eprintln!("usage: statfs-initfs-add <src> <dst> <path> <file> <octal-mode>");
        return ExitCode::FAILURE;
    }
    let (source, dest, path, file, mode) = (&args[1], &args[2], &args[3], &args[4], &args[5]);
    let mode = match u32::from_str_radix(mode.trim_start_matches("0o"), 8) {
        Ok(mode) => mode,
        Err(error) => {
            eprintln!("statfs-initfs-add: bad mode {mode}: {error}");
            return ExitCode::FAILURE;
        }
    };
    let image = match fs::read(source) {
        Ok(image) => image,
        Err(error) => {
            eprintln!("statfs-initfs-add: {source}: {error}");
            return ExitCode::FAILURE;
        }
    };
    let parsed = match Initfs::parse(&image) {
        Ok(parsed) => parsed,
        Err(error) => {
            eprintln!("statfs-initfs-add: {source}: {error:?}");
            return ExitCode::FAILURE;
        }
    };
    let mut builder = InitfsBuilder::new().with_image_epoch(parsed.image_epoch());
    for entry in parsed.entries() {
        if let Err(error) =
            builder.add_file_with_mtime(entry.path, entry.data, entry.mode, entry.mtime)
        {
            eprintln!("statfs-initfs-add: {}: {error:?}", entry.path);
            return ExitCode::FAILURE;
        }
    }
    let data = match fs::read(file) {
        Ok(data) => data,
        Err(error) => {
            eprintln!("statfs-initfs-add: {file}: {error}");
            return ExitCode::FAILURE;
        }
    };
    if let Err(error) = builder.add_file(path, &data, mode) {
        eprintln!("statfs-initfs-add: {path}: {error:?}");
        return ExitCode::FAILURE;
    }
    if let Err(error) = fs::write(dest, builder.build()) {
        eprintln!("statfs-initfs-add: {dest}: {error}");
        return ExitCode::FAILURE;
    }
    ExitCode::SUCCESS
}
