// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Command-line entry point for the conformance runner.

#![forbid(unsafe_code)]

use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use raven_tcpip_conformance::runner;
use raven_tcpip_conformance::table::{Table, Verdict, counts};

fn main() -> ExitCode {
    let mut all = false;
    let mut write_table = false;
    let mut filter: Option<String> = None;
    for argument in env::args().skip(1) {
        match argument.as_str() {
            "--all" => all = true,
            "--write-table" => write_table = true,
            "--help" | "-h" => {
                println!("usage: raven-tcpip-conformance [--all] [--filter SUBSTRING]");
                return ExitCode::SUCCESS;
            }
            other if other.starts_with("--filter=") => {
                filter = Some(other["--filter=".len()..].to_string());
            }
            other => {
                eprintln!("unknown argument {other}");
                return ExitCode::from(2);
            }
        }
    }
    if !all {
        // Running everything is the only supported mode; keep the flag for
        // compatibility with the ticket's acceptance command.
    }
    if write_table {
        return match write(filter.as_deref()) {
            Ok(()) => ExitCode::SUCCESS,
            Err(error) => {
                eprintln!("{error}");
                ExitCode::FAILURE
            }
        };
    }
    match run(filter.as_deref()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{error}");
            ExitCode::FAILURE
        }
    }
}

fn manifest_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn collect_scripts(root: &Path) -> Result<Vec<PathBuf>, String> {
    let mut scripts = Vec::new();
    let mut pending = vec![root.to_path_buf()];
    while let Some(directory) = pending.pop() {
        let entries = fs::read_dir(&directory)
            .map_err(|error| format!("read {}: {error}", directory.display()))?;
        for entry in entries {
            let entry = entry.map_err(|error| format!("read entry: {error}"))?;
            let path = entry.path();
            if path.is_dir() {
                pending.push(path);
            } else if path.extension().is_some_and(|extension| extension == "pkt") {
                scripts.push(path);
            }
        }
    }
    scripts.sort();
    Ok(scripts)
}

fn relative(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}

struct Meta {
    area: String,
    reference: String,
    skip: Option<String>,
    fail_note: Option<String>,
}

fn metadata(text: &str, path: &Path) -> Meta {
    let mut area = path
        .parent()
        .and_then(|parent| parent.file_name())
        .map(|name| name.to_string_lossy().to_string())
        .unwrap_or_else(|| "misc".to_string());
    let mut reference = "RFC 9293".to_string();
    let mut skip = None;
    let mut fail_note = None;
    for line in text.lines().take(20) {
        let line = line.trim();
        if let Some(value) = line.strip_prefix("# area:") {
            area = value.trim().to_string();
        } else if let Some(value) = line.strip_prefix("# ref:") {
            reference = value.trim().to_string();
        } else if let Some(value) = line.strip_prefix("# skip:") {
            skip = Some(value.trim().to_string());
        } else if let Some(value) = line.strip_prefix("# fail-note:") {
            fail_note = Some(value.trim().to_string());
        }
    }
    Meta {
        area,
        reference,
        skip,
        fail_note,
    }
}

fn write(filter: Option<&str>) -> Result<(), String> {
    let root = manifest_dir();
    let scripts_dir = root.join("scripts");
    let scripts = collect_scripts(&scripts_dir)?;
    let mut rows = Vec::new();
    for path in &scripts {
        let name = relative(&scripts_dir, path);
        if let Some(filter) = filter
            && !name.contains(filter)
        {
            continue;
        }
        let text = fs::read_to_string(path)
            .map_err(|error| format!("read {}: {error}", path.display()))?;
        let meta = metadata(&text, path);
        let (verdict, reason) = if let Some(reason) = meta.skip {
            (Verdict::Skip, reason)
        } else {
            match runner::run_text(&text) {
                Ok(()) => (Verdict::Pass, String::new()),
                Err(error) => (
                    Verdict::Fail,
                    meta.fail_note.unwrap_or_else(|| error.replace('\n', " ")),
                ),
            }
        };
        rows.push(raven_tcpip_conformance::table::Row {
            script: name,
            area: meta.area,
            reference: meta.reference,
            verdict,
            reason,
        });
    }
    let table = Table { rows };
    let (pass, fail, skip) = counts(&table.rows);
    let mut text = String::from(
        "# TCP conformance results\n\n\
         Fresh BSD-3-Clause scripts run through the C04 simulator. packetdrill\n\
         (GPL, tier C) is a tool whose script language we accept; no packetdrill\n\
         code or scripts are present. See `SCRIPTS.md` for the subset.\n\n",
    );
    text.push_str(&format!(
        "Totals: **{} scripts, {} pass, {} fail, {} skip.**\n\n",
        table.rows.len(),
        pass,
        fail,
        skip
    ));
    text.push_str(&table.render());
    fs::write(root.join("CONFORMANCE.md"), text)
        .map_err(|error| format!("write CONFORMANCE.md: {error}"))?;
    println!("wrote CONFORMANCE.md: {pass} pass / {fail} fail / {skip} skip");
    Ok(())
}

fn run(filter: Option<&str>) -> Result<(), String> {
    let root = manifest_dir();
    let scripts_dir = root.join("scripts");
    let table_path = root.join("CONFORMANCE.md");
    let table = Table::read(&table_path)?;
    let scripts = collect_scripts(&scripts_dir)?;
    let mut failures = Vec::new();
    let mut regressions = 0usize;
    let mut newly_passing = 0usize;
    let mut observed = Vec::new();
    for path in &scripts {
        let name = relative(&scripts_dir, path);
        if let Some(filter) = filter
            && !name.contains(filter)
        {
            continue;
        }
        let Some(row) = table.row(&name) else {
            failures.push(format!("{name}: missing from CONFORMANCE.md"));
            continue;
        };
        if row.verdict == Verdict::Skip {
            observed.push((name.clone(), row.area.clone(), false));
            continue;
        }
        let text = fs::read_to_string(path)
            .map_err(|error| format!("read {}: {error}", path.display()))?;
        let outcome = runner::run_text(&text);
        observed.push((name.clone(), row.area.clone(), outcome.is_ok()));
        match (row.verdict, outcome.is_ok()) {
            (Verdict::Pass, false) => {
                regressions += 1;
                failures.push(format!(
                    "{name}: marked pass but failed: {}",
                    outcome.expect_err("failed")
                ));
            }
            (Verdict::Fail, true) => {
                newly_passing += 1;
                println!(
                    "note: {name} is marked {} but now passes",
                    row.verdict.as_str()
                );
            }
            _ => {}
        }
    }
    for row in &table.rows {
        if !scripts
            .iter()
            .any(|path| relative(&scripts_dir, path) == row.script)
        {
            failures.push(format!(
                "{}: listed in CONFORMANCE.md but missing",
                row.script
            ));
        }
    }
    let (pass, fail, skip) = counts(&table.rows);
    println!(
        "scripts: {} run, table {} pass / {} fail / {} skip",
        observed.len(),
        pass,
        fail,
        skip
    );
    if newly_passing > 0 {
        println!("{newly_passing} known failure(s) now pass; update the table deliberately");
    }
    if !failures.is_empty() {
        for failure in &failures {
            eprintln!("FAIL {failure}");
        }
        return Err(format!(
            "{} failure(s), {} regression(s)",
            failures.len(),
            regressions
        ));
    }
    Ok(())
}
