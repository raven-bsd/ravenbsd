// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `libc-oracle` command-line entry point.

#![forbid(unsafe_code)]

use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match libc_oracle::run(&args) {
        Ok(code) => ExitCode::from(code),
        Err(error) => {
            eprintln!("libc-oracle: {error:#}");
            ExitCode::from(libc_oracle::exit_code(&error))
        }
    }
}
