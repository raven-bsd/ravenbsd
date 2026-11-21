// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! CLI for the RACK 218/218 acceptance audit.

use std::process::ExitCode;

use rack_acceptance::{audit, register, reports, source};
use rack_coverage::{Function, parse_functions};

fn main() -> ExitCode {
    let mut reports_dir = None;
    let mut donor = None;
    let mut rows = false;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--reports" => reports_dir = args.next(),
            "--donor" => donor = args.next(),
            "--rows" => rows = true,
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

    let donor = match load_donor(donor.as_deref()) {
        Ok(functions) => functions,
        Err(error) => {
            eprintln!("donor: {error}");
            return ExitCode::FAILURE;
        }
    };
    let register = match register::load_register() {
        Ok(rows) => rows,
        Err(error) => {
            eprintln!("register: {error}");
            return ExitCode::FAILURE;
        }
    };
    let source = match source::SourceIndex::load() {
        Ok(index) => index,
        Err(error) => {
            eprintln!("source: {error}");
            return ExitCode::FAILURE;
        }
    };
    let group_rows = reports_dir
        .as_deref()
        .map(|root| reports::load_group_rows(std::path::Path::new(root)))
        .unwrap_or_default();

    let report = audit::run(&donor, &register, &source, &group_rows);
    print!("{}", report.render());
    if rows {
        print!("{}", report.render_rows());
    }
    if report.is_ok() {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}

fn load_donor(path: Option<&str>) -> Result<Vec<Function>, String> {
    if let Some(path) = path {
        let text =
            std::fs::read_to_string(path).map_err(|error| format!("read {path}: {error}"))?;
        return Ok(parse_functions(&text));
    }
    register::load_donor()
}

fn usage() {
    eprintln!("usage: rack-acceptance [--reports <tickets-dir>] [--donor <rack.c>] [--rows]");
}
