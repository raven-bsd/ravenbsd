// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Generate the 10k RACK timer reference trace file.

use std::env;
use std::fs;
use std::path::Path;
use std::process::ExitCode;

use rack_timer_model::{TRACE_COUNT, render_trace};

fn generate(path: &str) -> Result<(), String> {
    if let Some(parent) = Path::new(path).parent() {
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    fs::write(path, render_trace()).map_err(|error| error.to_string())?;
    println!("wrote {TRACE_COUNT} traces to {path}");
    Ok(())
}

fn main() -> ExitCode {
    let mut args = env::args().skip(1);
    match (args.next().as_deref(), args.next()) {
        (Some("--generate"), Some(path)) => match generate(&path) {
            Ok(()) => ExitCode::SUCCESS,
            Err(error) => {
                eprintln!("rack-timer-model: {error}");
                ExitCode::FAILURE
            }
        },
        _ => {
            eprintln!("usage: rack-timer-model --generate <path>");
            ExitCode::from(2)
        }
    }
}
