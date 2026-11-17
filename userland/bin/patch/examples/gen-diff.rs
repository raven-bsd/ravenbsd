// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Test helper: print a unified diff using the first-party `raven-diff` engine.

use std::env;
use std::fs;
use std::process::ExitCode;

fn main() -> ExitCode {
    let arguments: Vec<String> = env::args().skip(1).collect();
    if arguments.len() != 2 {
        eprintln!("usage: gen-diff LEFT RIGHT");
        return ExitCode::from(2);
    }
    let left = match fs::read_to_string(&arguments[0]) {
        Ok(text) => text,
        Err(error) => {
            eprintln!("gen-diff: {}: {error}", arguments[0]);
            return ExitCode::FAILURE;
        }
    };
    let right = match fs::read_to_string(&arguments[1]) {
        Ok(text) => text,
        Err(error) => {
            eprintln!("gen-diff: {}: {error}", arguments[1]);
            return ExitCode::FAILURE;
        }
    };
    let (_, patch) = raven_diff::diff(&left, &right, true);
    print!("{patch}");
    ExitCode::SUCCESS
}
