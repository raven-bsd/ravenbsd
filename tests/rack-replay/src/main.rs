// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! CLI for the RACK replay and union audit.

use std::process::ExitCode;

use rack_coverage::parse_functions;
use rack_replay::{replay, union};

fn main() -> ExitCode {
    let mut replay_requested = false;
    let mut union_requested = false;
    let mut reports_dir = None;
    let mut donor = None;
    let mut manifest = None;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--replay" => replay_requested = true,
            "--union" => union_requested = true,
            "--reports" => reports_dir = args.next(),
            "--donor" => donor = args.next(),
            "--manifest" => manifest = args.next(),
            "--help" | "-h" => {
                usage();
                return ExitCode::SUCCESS;
            }
            other => {
                eprintln!("unknown argument: {other}");
                usage();
                return ExitCode::from(2);
            }
        }
    }
    if !replay_requested && !union_requested {
        replay_requested = true;
    }

    if replay_requested {
        match replay::replay_all() {
            Ok(report) => print!("{}", report.render()),
            Err(error) => {
                eprintln!("replay: {error}");
                return ExitCode::FAILURE;
            }
        }
    }

    if union_requested {
        let Some(root) = reports_dir else {
            eprintln!("--union requires --reports <tickets-directory>");
            return ExitCode::from(2);
        };
        let donor_functions = match load_donor(donor.as_deref(), manifest.as_deref()) {
            Ok(functions) => functions,
            Err(error) => {
                eprintln!("union: {error}");
                return ExitCode::FAILURE;
            }
        };
        let reports = union::load_group_reports(std::path::Path::new(&root));
        let report = union::audit(&reports, &donor_functions);
        print!("{}", report.render());
        if !report.is_ok() {
            return ExitCode::FAILURE;
        }
    }
    ExitCode::SUCCESS
}

fn load_donor(
    donor: Option<&str>,
    manifest: Option<&str>,
) -> Result<Vec<rack_coverage::Function>, String> {
    if let Some(path) = donor {
        let text =
            std::fs::read_to_string(path).map_err(|error| format!("donor {path}: {error}"))?;
        return Ok(parse_functions(&text));
    }
    let path = manifest.map_or_else(union::manifest_path, std::path::PathBuf::from);
    let text = std::fs::read_to_string(&path)
        .map_err(|error| format!("manifest {}: {error}", path.display()))?;
    Ok(rack_coverage::parse_manifest(&text))
}

fn usage() {
    eprintln!(
        "usage: rack-replay [--replay] [--union --reports <tickets-dir>] \
         [--donor <rack.c>] [--manifest <functions.txt>]"
    );
}
