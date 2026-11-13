// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `libc-census` command-line interface.

#![forbid(unsafe_code)]

use std::collections::{BTreeMap, BTreeSet};
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use anyhow::{Context, Result, anyhow, bail};

use libc_census::cells::{ScanOptions, cell_rows, donor_present, donor_roots, scan_donors};
use libc_census::merge::{MergeOptions, merge};
use libc_census::missing::{MissingOptions, run_missing};
use libc_census::model::{
    CellRow, DONORS, Kind, RunSummary, StandardEntry, StandardsOutput, SymlistSummary,
    SymlistsOutput, UndefOutput, UsesOutput,
};
use libc_census::ports::{PortsOptions, scan as scan_ports};
use libc_census::seed::seed;
use libc_census::standards::compute_ring;
use libc_census::symlists::{
    ManualEntry, ManualOutput, ParsedSymbols, parse_bionic, parse_freebsd, parse_illumos,
    parse_manual, parse_openbsd,
};
use libc_census::table15::{CompareOptions, compare};
use libc_census::universe::{Universe, read_names_file};
use libc_census::uses::{read_undef, scan_sources};

const USAGE: &str = "\
usage:
  libc-census run     [--resources DIR] [--donors a,b,..] [--aports DIR] [--pkgsrc DIR]
                      [--freebsd-ports DIR] [--openbsd-ports DIR] [--sources DIR] [--undef DIR]
                      [--names FILE] [--out DIR] [--jobs N] [--no-prefilter] [--rev TEXT]
  libc-census merge   [--out DIR] [--census libc/ravenlibc/abi/census] [--assignments FILE]
  libc-census missing --implemented FILE [--out DIR] [--min-packages 3] [--fail-p0]
  libc-census compare-table --table FILE [--resources DIR] [--require-all]
  libc-census seed-assignments --api-surface FILE [--out libc/ravenlibc/abi/census/assignments.toml]";

fn main() -> ExitCode {
    match run() {
        Ok(code) => ExitCode::from(code),
        Err(error) => {
            eprintln!("libc-census: {error:#}");
            ExitCode::from(2)
        }
    }
}

fn run() -> Result<u8> {
    let args: Vec<String> = env::args().skip(1).collect();
    let Some(command) = args.first().map(String::as_str) else {
        bail!("missing command\n{USAGE}");
    };
    match command {
        "run" => command_run(&args[1..]),
        "merge" => command_merge(&args[1..]),
        "missing" => command_missing(&args[1..]),
        "compare-table" => command_compare(&args[1..]),
        "seed-assignments" => command_seed(&args[1..]),
        "-h" | "--help" | "help" => {
            println!("{USAGE}");
            Ok(0)
        }
        other => bail!("unknown command `{other}`\n{USAGE}"),
    }
}

struct RunOptions {
    resources: PathBuf,
    donors: Vec<String>,
    aports: Option<PathBuf>,
    pkgsrc: Option<PathBuf>,
    freebsd_ports: Option<PathBuf>,
    openbsd_ports: Option<PathBuf>,
    sources: Option<PathBuf>,
    undef: Option<PathBuf>,
    names: Option<PathBuf>,
    out: PathBuf,
    jobs: usize,
    prefilter: bool,
    rev: String,
}

impl Default for RunOptions {
    fn default() -> Self {
        Self {
            resources: default_resources(),
            donors: DONORS.iter().map(|donor| (*donor).to_string()).collect(),
            aports: None,
            pkgsrc: None,
            freebsd_ports: None,
            openbsd_ports: None,
            sources: None,
            undef: None,
            names: None,
            out: PathBuf::from("tools/libc-census/out"),
            jobs: default_jobs(),
            prefilter: true,
            rev: env::var("LIBC_CENSUS_REV").unwrap_or_else(|_| "unversioned".to_string()),
        }
    }
}

fn command_run(args: &[String]) -> Result<u8> {
    let mut options = RunOptions::default();
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--resources" => options.resources = path_value(args, &mut index, "--resources")?,
            "--donors" => {
                let value = value(args, index + 1, "--donors")?;
                options.donors = value
                    .split(',')
                    .map(str::trim)
                    .filter(|donor| !donor.is_empty())
                    .map(str::to_string)
                    .collect();
                if options.donors.is_empty() {
                    bail!("--donors needs at least one donor\n{USAGE}");
                }
                for donor in &options.donors {
                    if !DONORS.contains(&donor.as_str()) {
                        bail!("unknown donor `{donor}`\n{USAGE}");
                    }
                }
                index += 1;
            }
            "--aports" => options.aports = Some(path_value(args, &mut index, "--aports")?),
            "--pkgsrc" => options.pkgsrc = Some(path_value(args, &mut index, "--pkgsrc")?),
            "--freebsd-ports" => {
                options.freebsd_ports = Some(path_value(args, &mut index, "--freebsd-ports")?)
            }
            "--openbsd-ports" => {
                options.openbsd_ports = Some(path_value(args, &mut index, "--openbsd-ports")?)
            }
            "--sources" => options.sources = Some(path_value(args, &mut index, "--sources")?),
            "--undef" => options.undef = Some(path_value(args, &mut index, "--undef")?),
            "--names" => options.names = Some(path_value(args, &mut index, "--names")?),
            "--out" => options.out = path_value(args, &mut index, "--out")?,
            "--jobs" => {
                let value = value(args, index + 1, "--jobs")?;
                options.jobs = value
                    .parse()
                    .ok()
                    .filter(|jobs| *jobs > 0)
                    .ok_or_else(|| anyhow!("--jobs needs a positive number\n{USAGE}"))?;
                index += 1;
            }
            "--no-prefilter" => options.prefilter = false,
            "--rev" => options.rev = value(args, index + 1, "--rev")?.to_string(),
            other => bail!("unexpected argument `{other}`\n{USAGE}"),
        }
        index += 1;
    }
    execute_run(&options)
}

fn execute_run(options: &RunOptions) -> Result<u8> {
    let mut summary = RunSummary {
        census_rev: options.rev.clone(),
        ..RunSummary::default()
    };
    let mut symlists = SymlistsOutput::default();
    let mut manual = ManualOutput::default();
    let mut universe_pairs: Vec<(String, Kind)> = Vec::new();

    let openbsd_list = options.resources.join("openbsd/lib/libc/Symbols.list");
    collect_list(
        "openbsd",
        &openbsd_list,
        parse_openbsd,
        &mut symlists,
        &mut universe_pairs,
        &mut summary,
    )?;

    let mut freebsd = ParsedSymbols::default();
    let mut freebsd_found = false;
    let mut freebsd_roots: Vec<&str> = donor_roots("freebsd").to_vec();
    freebsd_roots.push("freebsd/lib/msun");
    for root in freebsd_roots {
        let directory = options.resources.join(root);
        if !directory.is_dir() {
            continue;
        }
        for file in find_named(&directory, "Symbol.map")? {
            let parsed = libc_census::symlists::parse_file(&file, parse_freebsd)?;
            merge_parsed(&mut freebsd, parsed);
            freebsd_found = true;
        }
    }
    if freebsd_found {
        summary.inputs_found.push("freebsd Symbol.map".to_string());
        push_parsed("freebsd", &freebsd, &mut symlists, &mut universe_pairs);
    } else {
        summary
            .inputs_missing
            .push("freebsd Symbol.map".to_string());
    }

    let bionic_list = options.resources.join("bionic/libc/libc.map.txt");
    collect_list(
        "bionic",
        &bionic_list,
        parse_bionic,
        &mut symlists,
        &mut universe_pairs,
        &mut summary,
    )?;

    let illumos_list = options
        .resources
        .join("illumos-gate/usr/src/lib/libc/port/mapfile-vers");
    collect_list(
        "illumos",
        &illumos_list,
        parse_illumos,
        &mut symlists,
        &mut universe_pairs,
        &mut summary,
    )?;

    let manual_dir = options.resources.join("glibc/manual");
    if manual_dir.is_dir() {
        summary.inputs_found.push("glibc manual".to_string());
        for file in find_extension(&manual_dir, "texi")? {
            let text =
                fs::read_to_string(&file).with_context(|| format!("reading {}", file.display()))?;
            merge_manual(&mut manual, parse_manual(&text));
        }
        let names: BTreeSet<String> = manual.entries.keys().cloned().collect();
        symlists.sources.insert(
            "glibc".to_string(),
            SymlistSummary {
                count: names.len(),
                names: names.into_iter().collect(),
            },
        );
        for (name, entry) in &manual.entries {
            universe_pairs.push((name.clone(), entry.kind));
        }
    } else {
        summary.inputs_missing.push("glibc manual".to_string());
    }

    if let Some(path) = &options.names {
        if path.is_file() {
            summary.inputs_found.push("names file".to_string());
            for name in read_names_file(path)? {
                universe_pairs.push((name, Kind::Func));
            }
        } else {
            summary.inputs_missing.push("names file".to_string());
        }
    }

    let assignments_path = PathBuf::from("libc/ravenlibc/abi/census/assignments.toml");
    if assignments_path.is_file() {
        summary.inputs_found.push("assignments.toml".to_string());
        let text = fs::read_to_string(&assignments_path)
            .with_context(|| format!("reading {}", assignments_path.display()))?;
        let assignments: libc_census::model::Assignments = toml::from_str(&text)
            .with_context(|| format!("parsing {}", assignments_path.display()))?;
        for assignment in assignments.assign {
            let kind = assignment
                .kind
                .as_deref()
                .and_then(|kind| Kind::parse(kind).ok())
                .unwrap_or(Kind::Func);
            universe_pairs.push((assignment.name, kind));
        }
    }

    let universe = Universe::build(universe_pairs);
    let mut standards = StandardsOutput::default();
    for (name, entry) in &manual.entries {
        standards.entries.insert(
            name.clone(),
            StandardEntry {
                standard: entry.standard.iter().cloned().collect(),
                header: entry.header.clone(),
                kind: entry.kind.to_string(),
            },
        );
    }

    summary.donors = read_donor_revisions(&options.resources);
    for donor in &options.donors {
        let missing: Vec<String> = donor_roots(donor)
            .iter()
            .filter(|root| !options.resources.join(root).is_dir())
            .map(|root| (*root).to_string())
            .collect();
        if !missing.is_empty() {
            summary.missing_roots.insert(donor.clone(), missing);
        }
    }
    summary.complete = summary.missing_roots.is_empty();

    let scans = scan_donors(
        &options.resources,
        &universe,
        &ScanOptions {
            prefilter: options.prefilter,
            jobs: options.jobs,
            donors: options.donors.clone(),
        },
    )?;
    let rows = cell_rows(&universe, &scans, &options.donors);
    for (donor, scan) in &scans {
        summary.files.insert(donor.clone(), scan.files);
        summary.skipped_binary = summary.skipped_binary.saturating_add(scan.skipped_binary);
        summary.skipped_large = summary.skipped_large.saturating_add(scan.skipped_large);
    }

    let ring_map = computed_rings(&rows, &standards);
    let ports_options = PortsOptions {
        aports: options.aports.clone(),
        pkgsrc: options.pkgsrc.clone(),
        freebsd_ports: options.freebsd_ports.clone(),
        openbsd_ports: options.openbsd_ports.clone(),
    };
    let ports = scan_ports(&ports_options, &ring_map)?;
    for (snapshot, status) in &ports.snapshots {
        summary.snapshots.insert(
            snapshot.clone(),
            match &status.skipped {
                Some(reason) => reason.clone(),
                None => format!("packages: {}", status.packages),
            },
        );
    }
    record_snapshot_input("aports", &options.aports, &mut summary);
    record_snapshot_input("pkgsrc", &options.pkgsrc, &mut summary);
    record_snapshot_input("freebsd-ports", &options.freebsd_ports, &mut summary);
    record_snapshot_input("openbsd-ports", &options.openbsd_ports, &mut summary);

    let mut uses = match &options.sources {
        Some(root) => {
            if root.is_dir() {
                summary.inputs_found.push("sources tree".to_string());
            } else {
                summary.inputs_missing.push("sources tree".to_string());
            }
            scan_sources(root, &universe)?
        }
        None => UsesOutput {
            skipped: Some("skipped: no sources tree".to_string()),
            ..UsesOutput::default()
        },
    };
    uses.undef = Some(match &options.undef {
        Some(root) => {
            if root.is_dir() {
                summary.inputs_found.push("undef lists".to_string());
            } else {
                summary.inputs_missing.push("undef lists".to_string());
            }
            read_undef(root)?
        }
        None => UndefOutput {
            skipped: Some("skipped: no undefined-symbol lists".to_string()),
            ..UndefOutput::default()
        },
    });
    summary.undef = uses
        .undef
        .as_ref()
        .and_then(|undef| undef.skipped.clone())
        .unwrap_or_else(|| "scanned".to_string());

    summary.inputs_found.sort();
    summary.inputs_found.dedup();
    summary.inputs_missing.sort();
    summary.inputs_missing.dedup();

    fs::create_dir_all(&options.out)
        .with_context(|| format!("creating {}", options.out.display()))?;
    write_json(&options.out.join("cells.json"), &rows)?;
    write_json(&options.out.join("symlists.json"), &symlists)?;
    write_json(&options.out.join("standards.json"), &standards)?;
    write_json(&options.out.join("ports.json"), &ports)?;
    write_json(&options.out.join("uses.json"), &uses)?;
    write_json(&options.out.join("run-summary.json"), &summary)?;

    println!(
        "donors: {} present, {} with missing roots",
        options
            .donors
            .iter()
            .filter(|donor| donor_present(&options.resources, donor))
            .count(),
        summary.missing_roots.len()
    );
    println!("symbols: {}", rows.len());
    for donor in &options.donors {
        let scan = scans.get(donor);
        println!(
            "{}: {} file(s), {} skipped binary, {} skipped large",
            donor,
            scan.map_or(0, |scan| scan.files),
            scan.map_or(0, |scan| scan.skipped_binary),
            scan.map_or(0, |scan| scan.skipped_large)
        );
    }
    for (snapshot, status) in &summary.snapshots {
        println!("{snapshot}: {status}");
    }
    println!("complete: {}", summary.complete);
    Ok(0)
}

fn collect_list(
    name: &str,
    path: &Path,
    parser: fn(&str) -> ParsedSymbols,
    symlists: &mut SymlistsOutput,
    universe: &mut Vec<(String, Kind)>,
    summary: &mut RunSummary,
) -> Result<()> {
    if !path.is_file() {
        summary.inputs_missing.push(format!("{name} symbol list"));
        return Ok(());
    }
    summary.inputs_found.push(format!("{name} symbol list"));
    let parsed = libc_census::symlists::parse_file(path, parser)?;
    push_parsed(name, &parsed, symlists, universe);
    Ok(())
}

fn push_parsed(
    name: &str,
    parsed: &ParsedSymbols,
    symlists: &mut SymlistsOutput,
    universe: &mut Vec<(String, Kind)>,
) {
    symlists.sources.insert(
        name.to_string(),
        SymlistSummary {
            count: if parsed.identifier_lines > 0 {
                parsed.identifier_lines
            } else {
                parsed.names.len()
            },
            names: parsed.names.iter().cloned().collect(),
        },
    );
    for symbol in &parsed.names {
        universe.push((symbol.clone(), Kind::Func));
    }
}

fn merge_parsed(target: &mut ParsedSymbols, other: ParsedSymbols) {
    target.names.extend(other.names);
    target.private.extend(other.private);
    target.versions.extend(other.versions);
    target.notes.extend(other.notes);
    target.identifier_lines = target
        .identifier_lines
        .saturating_add(other.identifier_lines);
}

fn merge_manual(target: &mut ManualOutput, other: ManualOutput) {
    target.deftype_lines = target.deftype_lines.saturating_add(other.deftype_lines);
    target.standards_lines = target.standards_lines.saturating_add(other.standards_lines);
    target.one_field_standards = target
        .one_field_standards
        .saturating_add(other.one_field_standards);
    for (name, entry) in other.entries {
        let target_entry = target.entries.entry(name).or_insert(ManualEntry {
            kind: entry.kind,
            ..ManualEntry::default()
        });
        target_entry.standard.extend(entry.standard);
        if entry.header.is_some() {
            target_entry.header = entry.header;
        }
        if target_entry.kind == Kind::Func && entry.kind != Kind::Func {
            target_entry.kind = entry.kind;
        }
    }
}

fn computed_rings(rows: &[CellRow], standards: &StandardsOutput) -> BTreeMap<String, u8> {
    rows.iter()
        .map(|row| {
            let standard = standards
                .entries
                .get(&row.name)
                .map(|entry| entry.standard.clone())
                .unwrap_or_default();
            (
                row.name.clone(),
                compute_ring(&standard, &row.families).ring,
            )
        })
        .collect()
}

fn record_snapshot_input(name: &str, path: &Option<PathBuf>, summary: &mut RunSummary) {
    match path {
        Some(path) if path.is_dir() => summary.inputs_found.push(format!("{name} snapshot")),
        Some(_) => summary.inputs_missing.push(format!("{name} snapshot")),
        None => summary
            .inputs_missing
            .push(format!("{name} snapshot (not given)")),
    }
}

fn read_donor_revisions(resources: &Path) -> BTreeMap<String, String> {
    let path = resources.join("donors.lock");
    let mut revisions = BTreeMap::new();
    let Ok(text) = fs::read_to_string(&path) else {
        return revisions;
    };
    for line in text.lines() {
        let mut fields = line.split_whitespace();
        let Some(name) = fields.next() else {
            continue;
        };
        let Some(revision) = fields.next() else {
            continue;
        };
        if DONORS.contains(&name) {
            revisions.insert(name.to_string(), revision.to_string());
        }
    }
    revisions
}

fn command_merge(args: &[String]) -> Result<u8> {
    let mut options = MergeOptions::default();
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--out" => options.out = path_value(args, &mut index, "--out")?,
            "--census" => options.census = path_value(args, &mut index, "--census")?,
            "--assignments" => {
                options.assignments = Some(path_value(args, &mut index, "--assignments")?)
            }
            other => bail!("unexpected argument `{other}`\n{USAGE}"),
        }
        index += 1;
    }
    let report = merge(&options)?;
    println!(
        "symbols: {} rows, complete: {}",
        report.symbols, report.complete
    );
    for (ring, count) in &report.rings {
        println!("ring {ring}: {count}");
    }
    println!("ring differences: {}", report.ring_diff);
    Ok(0)
}

fn command_missing(args: &[String]) -> Result<u8> {
    let mut options = MissingOptions::default();
    let mut fail_p0 = false;
    let mut implemented = None;
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--implemented" => implemented = Some(path_value(args, &mut index, "--implemented")?),
            "--out" => options.out = path_value(args, &mut index, "--out")?,
            "--min-packages" => {
                let value = value(args, index + 1, "--min-packages")?;
                options.min_packages = value
                    .parse()
                    .map_err(|_| anyhow!("--min-packages needs a number\n{USAGE}"))?;
                index += 1;
            }
            "--fail-p0" => fail_p0 = true,
            other => bail!("unexpected argument `{other}`\n{USAGE}"),
        }
        index += 1;
    }
    options.implemented =
        implemented.ok_or_else(|| anyhow!("--implemented is required\n{USAGE}"))?;
    let report = run_missing(&options)?;
    println!("class\tname\tpackages");
    for row in &report.rows {
        println!("{}\t{}\t{}", row.class.as_str(), row.name, row.packages);
    }
    eprintln!("package data: {}", report.source);
    if fail_p0 && report.p0_nonempty {
        return Ok(1);
    }
    Ok(0)
}

fn command_compare(args: &[String]) -> Result<u8> {
    let mut options = CompareOptions::default();
    let mut table = None;
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--table" => table = Some(path_value(args, &mut index, "--table")?),
            "--resources" => options.resources = path_value(args, &mut index, "--resources")?,
            "--require-all" => options.require_all = true,
            other => bail!("unexpected argument `{other}`\n{USAGE}"),
        }
        index += 1;
    }
    options.table = table.ok_or_else(|| anyhow!("--table is required\n{USAGE}"))?;
    let report = compare(&options)?;
    println!(
        "symbol rows: {}, header rows: {}",
        report.symbol_rows, report.header_rows
    );
    println!(
        "donors: {} present, {} missing",
        report.present_donors.len(),
        report.missing_donors.len()
    );
    if report.partial {
        println!("partial: missing {}", report.missing_donors.join(", "));
    }
    println!("symbol differences: {}", report.symbol_differences.len());
    println!("header differences: {}", report.header_differences.len());
    for difference in report
        .symbol_differences
        .iter()
        .chain(report.header_differences.iter())
    {
        println!(
            "{}\t{}\t{}\t{}\t{}",
            if difference.header {
                "header"
            } else {
                "symbol"
            },
            difference.key,
            difference.donor,
            difference.doc,
            difference.tool
        );
    }
    println!(
        "result: {}",
        match report.status {
            libc_census::model::Exit::Ok => "PASS",
            libc_census::model::Exit::GateFailed => "FAIL",
            libc_census::model::Exit::Usage => "PARTIAL (--require-all)",
        }
    );
    Ok(report.status.code())
}

fn command_seed(args: &[String]) -> Result<u8> {
    let mut api_surface = None;
    let mut out = PathBuf::from("libc/ravenlibc/abi/census/assignments.toml");
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--api-surface" => api_surface = Some(path_value(args, &mut index, "--api-surface")?),
            "--out" => out = path_value(args, &mut index, "--out")?,
            other => bail!("unexpected argument `{other}`\n{USAGE}"),
        }
        index += 1;
    }
    let api_surface = api_surface.ok_or_else(|| anyhow!("--api-surface is required\n{USAGE}"))?;
    let result = seed(&api_surface, &out)?;
    for conflict in &result.conflicts {
        eprintln!("conflict: {conflict}");
    }
    println!(
        "rows: {}, assignments: {}, skipped patterns: {}",
        result.rows,
        result.assignments.assign.len(),
        result.skipped_patterns
    );
    Ok(0)
}

fn default_resources() -> PathBuf {
    let current = env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let base = current.canonicalize().unwrap_or_else(|_| current.clone());
    let mut directory = Some(base.as_path());
    while let Some(path) = directory {
        let candidate = path.join("ravenbsd-resources");
        if candidate.is_dir() {
            return candidate;
        }
        directory = path.parent();
    }
    base.parent().unwrap_or(&base).join("ravenbsd-resources")
}

fn default_jobs() -> usize {
    std::thread::available_parallelism()
        .map(std::num::NonZeroUsize::get)
        .unwrap_or(1)
        .min(8)
}

fn write_json<T: serde::Serialize>(path: &Path, value: &T) -> Result<()> {
    let mut text = serde_json::to_string_pretty(value)
        .with_context(|| format!("serialising {}", path.display()))?;
    text.push('\n');
    fs::write(path, text).with_context(|| format!("writing {}", path.display()))
}

fn find_named(root: &Path, name: &str) -> Result<Vec<PathBuf>> {
    let mut found = Vec::new();
    walk(root, &mut |path| {
        if path.file_name().and_then(|value| value.to_str()) == Some(name) {
            found.push(path.to_path_buf());
        }
    })?;
    found.sort();
    Ok(found)
}

fn find_extension(root: &Path, extension: &str) -> Result<Vec<PathBuf>> {
    let mut found = Vec::new();
    walk(root, &mut |path| {
        if path.extension().and_then(|value| value.to_str()) == Some(extension) {
            found.push(path.to_path_buf());
        }
    })?;
    found.sort();
    Ok(found)
}

fn walk(root: &Path, visit: &mut impl FnMut(&Path)) -> Result<()> {
    let mut entries = Vec::new();
    for entry in fs::read_dir(root).with_context(|| format!("reading {}", root.display()))? {
        entries.push(entry?);
    }
    entries.sort_by_key(|entry| entry.file_name());
    for entry in entries {
        let name = entry.file_name();
        let Some(name) = name.to_str() else {
            continue;
        };
        if name.starts_with('.') {
            continue;
        }
        let path = entry.path();
        if entry.file_type()?.is_dir() {
            walk(&path, visit)?;
        } else {
            visit(&path);
        }
    }
    Ok(())
}

fn value<'a>(args: &'a [String], index: usize, flag: &str) -> Result<&'a str> {
    args.get(index)
        .map(String::as_str)
        .ok_or_else(|| anyhow!("{flag} needs a value\n{USAGE}"))
}

fn path_value(args: &[String], index: &mut usize, flag: &str) -> Result<PathBuf> {
    *index += 1;
    let value = value(args, *index, flag)?;
    Ok(PathBuf::from(value))
}

#[cfg(test)]
#[path = "main_ut.rs"]
mod tests;
