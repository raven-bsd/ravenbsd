// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Generate or verify the 10k RACK reference trace file.

use std::env;
use std::fs;
use std::process::ExitCode;

use rack_loss_model::{Outcome, Scenario, TRACE_COUNT, run, trace_row};

const HEADER: &str = "\
# RACK loss/reordering reference traces (K319)
# seed\tevents\tlost\tpipe\tretransmits\trecovery_entries/exits\tevent_digest
";

fn generate(path: &str) -> Result<(), String> {
    let mut text = String::from(HEADER);
    for seed in 1..=TRACE_COUNT {
        let scenario = Scenario::generate(seed);
        let outcome: Outcome = run(&scenario);
        text.push_str(&trace_row(&scenario, &outcome));
        text.push('\n');
    }
    if let Some(parent) = std::path::Path::new(path).parent() {
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    fs::write(path, text).map_err(|error| error.to_string())?;
    println!("wrote {TRACE_COUNT} traces to {path}");
    Ok(())
}

fn main() -> ExitCode {
    let mut args = env::args().skip(1);
    match (args.next().as_deref(), args.next()) {
        (Some("--generate"), Some(path)) => match generate(&path) {
            Ok(()) => ExitCode::SUCCESS,
            Err(error) => {
                eprintln!("rack-loss-model: {error}");
                ExitCode::FAILURE
            }
        },
        _ => {
            eprintln!("usage: rack-loss-model --generate <path>");
            ExitCode::from(2)
        }
    }
}
