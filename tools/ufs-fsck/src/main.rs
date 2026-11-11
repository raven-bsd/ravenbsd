// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `ufs-fsck` command line.

#![forbid(unsafe_code)]

use std::env;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use ufs_core::check::{Finding, Mode, Report, check};
use ufs_fsck::FileDisk;

const USAGE: &str = "\
usage: ufs-fsck [-n] [-p] image

  -n  check only, never write (the default)
  -p  preen: repair the normal inconsistencies and verify the result

exit status: 0 clean, 1 repaired (or repairable), 4 unrepairable, 8 usage
";

fn main() -> ExitCode {
    match run(env::args().skip(1).collect()) {
        Ok(code) => ExitCode::from(code),
        Err(error) => {
            eprintln!("ufs-fsck: {error}");
            ExitCode::from(8)
        }
    }
}

fn run(args: Vec<String>) -> Result<u8, String> {
    if args.iter().any(|arg| arg == "-h" || arg == "--help") {
        print!("{USAGE}");
        return Ok(0);
    }
    let mut repair = false;
    let mut read_only = false;
    let mut image = None;
    for arg in args {
        match arg.as_str() {
            "-n" => read_only = true,
            "-p" => repair = true,
            other if other.starts_with('-') => return Err(format!("unknown option {other}")),
            _ => {
                if image.is_some() {
                    return Err("only one image path is accepted".to_string());
                }
                image = Some(PathBuf::from(arg));
            }
        }
    }
    if repair && read_only {
        return Err("-n and -p are mutually exclusive".to_string());
    }
    let image = image.ok_or("missing image path")?;
    let mode = if repair { Mode::Repair } else { Mode::ReadOnly };
    let mut disk = FileDisk::open(&image, repair)?;
    let report = match check(&mut disk, mode) {
        Ok(report) => report,
        Err(error) => {
            eprintln!("ufs-fsck: {}: {error}", image.display());
            return Ok(4);
        }
    };
    print_report(&image, &report);
    if !report.clean_after {
        Ok(4)
    } else if report.fixed.is_empty() {
        Ok(0)
    } else {
        Ok(1)
    }
}

fn print_report(image: &Path, report: &Report) {
    for finding in &report.findings {
        println!("{}", format_finding(finding));
    }
    if report.clean_after {
        if report.fixed.is_empty() {
            println!("{}: clean", image.display());
        } else {
            println!(
                "{}: repaired {} finding(s), clean",
                image.display(),
                report.fixed.len()
            );
        }
    } else {
        println!(
            "{}: {} unresolved finding(s)",
            image.display(),
            report.findings.len().saturating_sub(report.fixed.len())
        );
    }
}

fn format_finding(finding: &Finding) -> String {
    let mut text = format!("phase {}: {:?}", finding.phase, finding.problem);
    if let Some(inode) = finding.inode {
        text.push_str(&format!(" inode {inode}"));
    }
    if let Some(block) = finding.block {
        text.push_str(&format!(" block {block}"));
    }
    text
}
