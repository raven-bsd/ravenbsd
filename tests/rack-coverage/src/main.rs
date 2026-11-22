// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! CLI for the RACK donor-function coverage checker.

use std::process::ExitCode;

use rack_coverage::{check, parse_functions, parse_manifest, parse_matrix};

fn main() -> ExitCode {
    let mut donor = None;
    let mut matrix = None;
    let mut manifest = None;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--donor" => donor = args.next(),
            "--matrix" => matrix = args.next(),
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
    let (Some(donor_path), Some(matrix_path)) = (donor, matrix) else {
        usage();
        return ExitCode::from(2);
    };
    let donor_text = match std::fs::read_to_string(&donor_path) {
        Ok(text) => text,
        Err(error) => {
            eprintln!("donor {donor_path}: {error}");
            return ExitCode::FAILURE;
        }
    };
    let matrix_text = match std::fs::read_to_string(&matrix_path) {
        Ok(text) => text,
        Err(error) => {
            eprintln!("matrix {matrix_path}: {error}");
            return ExitCode::FAILURE;
        }
    };
    let functions = parse_functions(&donor_text);
    let rows = parse_matrix(&matrix_text);
    println!("donor: {} function(s) ({donor_path})", functions.len());
    println!("matrix: {} row(s) ({matrix_path})", rows.len());

    let mut failed = false;
    if let Some(manifest_path) = manifest {
        let manifest_text = match std::fs::read_to_string(&manifest_path) {
            Ok(text) => text,
            Err(error) => {
                eprintln!("manifest {manifest_path}: {error}");
                return ExitCode::FAILURE;
            }
        };
        let expected = parse_manifest(&manifest_text);
        if functions != expected {
            failed = true;
            eprintln!(
                "manifest mismatch: parsed {} donor function(s), manifest has {}",
                functions.len(),
                expected.len()
            );
            for (index, (parsed, expected)) in functions.iter().zip(expected.iter()).enumerate() {
                if parsed != expected {
                    eprintln!(
                        "first difference at {index}: parsed {}:{}, manifest {}:{}",
                        parsed.line, parsed.name, expected.line, expected.name
                    );
                    break;
                }
            }
            if functions.len() != expected.len() {
                eprintln!(
                    "function count differs: parsed {} vs manifest {}",
                    functions.len(),
                    expected.len()
                );
            }
        } else {
            println!("manifest: {} entries match", expected.len());
        }
    }

    let report = check(&functions, &rows);
    for error in report.errors() {
        eprintln!("{error}");
    }
    if report.is_ok() && !failed {
        println!("result: PASS");
        ExitCode::SUCCESS
    } else {
        println!(
            "result: FAIL ({} missing, {} duplicate, {} unknown, {} empty Rust, {} empty test, {} empty mutation, {} not PASS)",
            report.missing.len(),
            report.duplicates.len(),
            report.unknown.len(),
            report.empty_rust.len(),
            report.empty_test.len(),
            report.empty_mutation.len(),
            report.not_pass.len()
        );
        ExitCode::FAILURE
    }
}

fn usage() {
    eprintln!(
        "usage: rack-coverage --donor <rack.c> --matrix <coverage.md> [--manifest <functions.txt>]"
    );
}
