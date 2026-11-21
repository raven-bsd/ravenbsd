// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! CLI for the RACK register repair/check.

use std::process::ExitCode;

use rack_acceptance::register::{load_donor, load_register};
use rack_register::check;
use rack_register::propose;
use rack_register::source::SourceIndex;

fn main() -> ExitCode {
    let mut reports = None;
    let mut mode = "check";
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--check" => mode = "check",
            "--propose" => mode = "propose",
            "--propose-check" => mode = "propose-check",
            "--reports" => reports = args.next(),
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

    let donor = match load_donor() {
        Ok(donor) => donor,
        Err(error) => {
            eprintln!("donor: {error}");
            return ExitCode::FAILURE;
        }
    };
    let source = match SourceIndex::load() {
        Ok(source) => source,
        Err(error) => {
            eprintln!("source: {error}");
            return ExitCode::FAILURE;
        }
    };

    if mode == "propose" || mode == "propose-check" {
        let rows = match propose::propose_from_repo(reports.as_deref().map(std::path::Path::new)) {
            Ok(rows) => rows,
            Err(error) => {
                eprintln!("propose: {error}");
                return ExitCode::FAILURE;
            }
        };
        if mode == "propose-check" {
            let report = check::check(&donor, &rows, &source);
            print!("{}", report.render());
            return if report.is_ok() {
                ExitCode::SUCCESS
            } else {
                ExitCode::FAILURE
            };
        }
        print!("{}", propose::render(&rows, &donor));
        return ExitCode::SUCCESS;
    }

    let rows = match load_register() {
        Ok(rows) => rows,
        Err(error) => {
            eprintln!("register: {error}");
            return ExitCode::FAILURE;
        }
    };
    let report = check::check(&donor, &rows, &source);
    print!("{}", report.render());
    if report.is_ok() {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}

fn usage() {
    eprintln!("usage: rack-register [--check | --propose [--reports <tickets-dir>]]");
}
