// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `libc-size` command-line interface (`C69` 4.2).
//!
//! Exit codes: 0 ok, 1 a gate failed, 2 usage or I/O, 77 a required tool is missing.

#![forbid(unsafe_code)]

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use anyhow::{Context, Result, bail};

use libc_size::budget::{Budgets, check};
use libc_size::build::{self, CommandSpec};
use libc_size::measure::{
    dynamic_info, file_size, parse_archive_sizes, parse_berkeley, parse_map_members,
    parse_nm_sizes, parse_sections,
};
use libc_size::report::{BootRow, Results, Row, SetSymbolRow, SkipPath, Toolchain, TopSymbols};
use libc_size::rules::{self, RuleContext};
use libc_size::tools::{MissingTool, Tools, required};
use libc_size::{Arch, Lang, Linkage, Set};

#[cfg(test)]
#[path = "main_ut.rs"]
mod tests;

const USAGE: &str = "\
usage:
  libc-size run       [--arch x86_64,aarch64,riscv64|all] [--linkage static,dynamic|all]
                      [--set tiny,server|all] [--lang rust,c|all] [--probe P0,P1,...|all]
                      [--libc-dir DIR] [--out DIR] [--no-build-libc] [--check]
  libc-size check     [--budgets FILE] [--results FILE] [--allow-missing]
  libc-size freeze    --results FILE [--out FILE] [--headroom-percent N]
  libc-size diff      --old FILE --new FILE
  libc-size members   --map FILE
  libc-size boot      [--arch A] [--libc-dir DIR] [--out DIR] [--json]
  libc-size relocs    --so FILE
  libc-size monomorph --lib FILE [--allowlist FILE]

Every command accepts --root DIR (default: the current directory).";

fn main() -> ExitCode {
    match dispatch(std::env::args().skip(1).collect()) {
        Ok(code) => ExitCode::from(code),
        Err(error) => {
            if let Some(missing) = error.downcast_ref::<MissingTool>() {
                eprintln!("skipped: {missing}");
                if required() {
                    eprintln!("libc-size: RL_SIZE_REQUIRED=1 turns the skip into a failure");
                    return ExitCode::from(1);
                }
                return ExitCode::from(77);
            }
            eprintln!("libc-size: {error:#}");
            ExitCode::from(2)
        }
    }
}

fn dispatch(args: Vec<String>) -> Result<u8> {
    let Some(command) = args.first().map(String::as_str) else {
        bail!("missing command\n{USAGE}");
    };
    let rest = &args[1..];
    match command {
        "run" => command_run(rest),
        "check" => command_check(rest),
        "freeze" => command_freeze(rest),
        "diff" => command_diff(rest),
        "members" => command_members(rest),
        "boot" => command_boot(rest),
        "relocs" => command_relocs(rest),
        "monomorph" => command_monomorph(rest),
        "-h" | "--help" | "help" => {
            println!("{USAGE}");
            Ok(0)
        }
        other => bail!("unknown command `{other}`\n{USAGE}"),
    }
}

fn root_from(args: &[String]) -> PathBuf {
    if let Some(index) = args.iter().position(|arg| arg == "--root")
        && let Some(value) = args.get(index + 1)
    {
        return PathBuf::from(value);
    }
    std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
}

fn results_path(root: &Path) -> PathBuf {
    root.join("build/size/libc-size.json")
}

fn budgets_path(root: &Path) -> PathBuf {
    root.join("tests/libc/size/BUDGETS.toml")
}

struct RunOptions {
    root: PathBuf,
    arches: Vec<Arch>,
    linkages: Vec<Linkage>,
    sets: Vec<Set>,
    langs: Vec<Lang>,
    probes: Vec<&'static str>,
    libc_override: Option<PathBuf>,
    out: PathBuf,
    no_build_libc: bool,
    check_after: bool,
}

fn parse_run_args(args: &[String]) -> Result<RunOptions> {
    let mut options = RunOptions {
        root: root_from(args),
        arches: Arch::all().to_vec(),
        linkages: vec![Linkage::Static, Linkage::Dynamic],
        sets: vec![Set::Tiny, Set::Server],
        langs: vec![Lang::Rust],
        probes: libc_size::probe::probe_ids(),
        libc_override: None,
        out: PathBuf::from("build/size"),
        no_build_libc: false,
        check_after: false,
    };
    if !options.out.is_absolute() {
        options.out = options.root.join(&options.out);
    }
    let mut index = 0usize;
    while index < args.len() {
        match args.get(index).map(String::as_str) {
            Some("--arch") => {
                index += 1;
                options.arches = Arch::parse_list(value(args, index, "--arch")?)?;
            }
            Some("--linkage") => {
                index += 1;
                options.linkages = Linkage::parse_list(value(args, index, "--linkage")?)?;
            }
            Some("--set") => {
                index += 1;
                options.sets = Set::parse_list(value(args, index, "--set")?)?;
            }
            Some("--lang") => {
                index += 1;
                options.langs = Lang::parse_list(value(args, index, "--lang")?)?;
            }
            Some("--probe") => {
                index += 1;
                options.probes = libc_size::probe::parse_selection(value(args, index, "--probe")?)?;
            }
            Some("--libc-dir") => {
                index += 1;
                options.libc_override = Some(PathBuf::from(value(args, index, "--libc-dir")?));
            }
            Some("--out") => {
                index += 1;
                let out = PathBuf::from(value(args, index, "--out")?);
                options.out = if out.is_absolute() {
                    out
                } else {
                    options.root.join(out)
                };
            }
            Some("--no-build-libc") => options.no_build_libc = true,
            Some("--check") => options.check_after = true,
            Some("--root") => index += 1,
            Some(other) => bail!("unknown run flag `{other}`"),
            None => break,
        }
        index += 1;
    }
    Ok(options)
}

fn value<'a>(args: &'a [String], index: usize, flag: &str) -> Result<&'a str> {
    args.get(index)
        .map(String::as_str)
        .with_context(|| format!("{flag} needs a value"))
}

fn command_run(args: &[String]) -> Result<u8> {
    let options = parse_run_args(args)?;
    let tools = Tools::discover()?;
    let results = measure(&tools, &options)?;
    let json_path = options.out.join("libc-size.json");
    let md_path = options.out.join("libc-size.md");
    results.write(&json_path)?;
    write_text(&md_path, &libc_size::report::render_markdown(&results))?;
    println!(
        "libc-size run: {} rows, {} rules, {} invariants, {} boot rows, {} skips",
        results.rows.len(),
        results.rules.len(),
        results.invariants.len(),
        results.boot.len(),
        results.skips.len()
    );
    println!(
        "libc-size run: wrote {} and {}",
        json_path.display(),
        md_path.display()
    );
    if options.check_after {
        let budgets = budgets_path(&options.root);
        if budgets.is_file() {
            let parsed = Budgets::read(&budgets)?;
            let report = check(&results, &parsed, false);
            print!("{}", report.render());
            return Ok(if report.passed() { 0 } else { 1 });
        }
        println!("libc-size run: no BUDGETS.toml yet; check reports `no budget`");
        return Ok(1);
    }
    Ok(0)
}

struct Extra {
    members: Vec<String>,
    symbols: Vec<String>,
    init_array: u64,
    top: Vec<String>,
}

fn measure(tools: &Tools, options: &RunOptions) -> Result<Results> {
    let mut results = Results::new(toolchain(tools));
    let rules_file = rules::load(&options.root.join("tests/libc/size/rules.toml")).ok();
    let mut extras: BTreeMap<String, Extra> = BTreeMap::new();
    let mut member_map: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut set_dirs: BTreeMap<(Arch, Set), PathBuf> = BTreeMap::new();
    for arch in &options.arches {
        for set in &options.sets {
            let dir = prepare_set(tools, options, *arch, *set);
            match dir {
                Ok(path) => {
                    set_dirs.insert((*arch, *set), path);
                }
                Err(error) => {
                    let reason = format!("libc-pack failed: {error}");
                    for linkage in &options.linkages {
                        for lang in &options.langs {
                            for probe in &options.probes {
                                results.rows.push(Row::skipped(
                                    probe, *arch, *linkage, *set, *lang, &reason,
                                ));
                            }
                        }
                    }
                }
            }
        }
        for linkage in &options.linkages {
            for set in &options.sets {
                if *linkage == Linkage::Dynamic && *set == Set::Tiny {
                    for lang in &options.langs {
                        for probe in &options.probes {
                            results.rows.push(Row::skipped(
                                probe,
                                *arch,
                                *linkage,
                                *set,
                                *lang,
                                "dynamic tiny does not exist (tiny is static)",
                            ));
                        }
                    }
                    continue;
                }
                let libc_dir = set_dirs
                    .get(&(*arch, *set))
                    .cloned()
                    .unwrap_or_else(|| options.root.join(format!("build/{}", arch.short())));
                if *linkage == Linkage::Dynamic && !libc_dir.join("libc.so.1").is_file() {
                    let path = format!("build/{}/libc.so.1", arch.short());
                    for lang in &options.langs {
                        for probe in &options.probes {
                            results.rows.push(Row::skipped(
                                probe,
                                *arch,
                                *linkage,
                                *set,
                                *lang,
                                libc_size::relocs::absence_reason(),
                            ));
                            results.skips.push(SkipPath {
                                row: format!(
                                    "{} {} {} {}",
                                    probe,
                                    arch.short(),
                                    linkage.as_str(),
                                    lang.as_str()
                                ),
                                path: path.clone(),
                                reason: libc_size::relocs::SKIPPED_ABSENT
                                    .trim_start_matches("skipped: ")
                                    .to_string(),
                            });
                        }
                    }
                    continue;
                }
                let symbols = archive_symbols(tools, &libc_dir.join("libc.a"), &options.root).ok();
                for lang in &options.langs {
                    if *lang == Lang::C && !tools.has_c_toolchain() {
                        for probe in &options.probes {
                            results.rows.push(Row::skipped(
                                probe,
                                *arch,
                                *linkage,
                                *set,
                                *lang,
                                "no C toolchain",
                            ));
                        }
                        continue;
                    }
                    for probe in &options.probes {
                        if *lang == Lang::C && *probe == "P9" {
                            results.rows.push(Row::skipped(
                                probe,
                                *arch,
                                *linkage,
                                *set,
                                *lang,
                                "P9 is the Rust full link (generated p9_all.rs)",
                            ));
                            continue;
                        }
                        let outcome = measure_probe(
                            tools,
                            options,
                            *arch,
                            *linkage,
                            *set,
                            *lang,
                            probe,
                            &libc_dir,
                            symbols.as_ref(),
                        );
                        match outcome {
                            Ok((row, extra)) => {
                                let key = rules::cell_key(*arch, *linkage, *set, *lang);
                                member_map.insert(key.clone(), extra.members.clone());
                                if (row.probe == "P2" || row.probe == "P9") && !extra.top.is_empty()
                                {
                                    results.top_symbols.push(TopSymbols {
                                        probe: row.probe.clone(),
                                        arch: row.arch,
                                        linkage: row.linkage,
                                        set: row.set,
                                        lang: row.lang,
                                        symbols: extra.top.clone(),
                                    });
                                }
                                extras.insert(format!("{key}/{}", row.probe), extra);
                                results.rows.push(row);
                            }
                            Err(error) => {
                                results.rows.push(Row::skipped(
                                    probe,
                                    *arch,
                                    *linkage,
                                    *set,
                                    *lang,
                                    &sanitize_reason(&format!("{error:#}"), &options.root),
                                ));
                            }
                        }
                    }
                }
            }
        }
        results.set_symbols.push(compare_sets(
            tools,
            &options.root,
            *arch,
            set_dirs.get(&(*arch, Set::Tiny)).map(PathBuf::as_path),
            set_dirs.get(&(*arch, Set::Server)).map(PathBuf::as_path),
        ));
        results.libc_so.push(libc_size::relocs::measure(
            tools,
            &options.root,
            *arch,
            &options.root.join(format!("build/{}", arch.short())),
            &format!("build/{}/libc.so.1", arch.short()),
        )?);
        if let Some(row) = boot_row(tools, options, *arch, &set_dirs) {
            results.boot.push(row);
        }
    }
    if let Some(file) = rules_file {
        evaluate_rules(&file, &results.rows, &extras, &mut results.rules);
    }
    evaluate_large_members(tools, options, &set_dirs, &mut results.rules);
    results.invariants = rules::evaluate_row_invariants(&results.rows);
    results.invariants.extend(rules::evaluate_member_invariants(
        &results.rows,
        &member_map,
    ));
    results
        .invariants
        .extend(rules::evaluate_libc_so_invariants(&results.libc_so));
    results.finalize();
    Ok(results)
}

fn toolchain(tools: &Tools) -> Toolchain {
    tools.toolchain()
}

fn prepare_set(tools: &Tools, options: &RunOptions, arch: Arch, set: Set) -> Result<PathBuf> {
    let source = options
        .libc_override
        .clone()
        .unwrap_or_else(|| options.root.join(format!("build/{}", arch.short())));
    if !options.no_build_libc {
        // `C60`'s `libc-pack` uses `llvm-ar rcsD`, which adds to an existing archive. A stale
        // `libc.a` from an earlier set would leave members behind (duplicate strong symbols);
        // remove it so every set is packed from exactly one staticlib.
        let _ = std::fs::remove_file(source.join("libc.a"));
        build::ensure_libc(tools, &options.root, arch, set)?;
    }
    let dest = options.out.join(arch.short()).join(set.as_str());
    std::fs::create_dir_all(&dest)?;
    for name in ["libc.a", "crt1.o", "Scrt1.o", "crti.o", "crtn.o"] {
        let from = source.join(name);
        if from.is_file() {
            std::fs::copy(&from, dest.join(name))
                .with_context(|| format!("cannot copy {}", from.display()))?;
        }
    }
    Ok(dest)
}

#[allow(clippy::too_many_arguments)]
fn measure_probe(
    tools: &Tools,
    options: &RunOptions,
    arch: Arch,
    linkage: Linkage,
    set: Set,
    lang: Lang,
    probe: &str,
    libc_dir: &Path,
    archive_symbols: Option<&BTreeSet<String>>,
) -> Result<(Row, Extra)> {
    if let Some(symbols) = archive_symbols
        && let Some(definition) = libc_size::probe::find(probe)
        && let Some(missing) = definition
            .symbols
            .iter()
            .find(|symbol| !symbols.contains(**symbol))
    {
        bail!("undefined symbol: {missing}");
    }
    let out_dir = options
        .out
        .join(arch.short())
        .join(set.as_str())
        .join(linkage.as_str())
        .join(lang.as_str());
    let p9_symbols = if probe == "P9" {
        p9_symbols(&options.root)?
    } else {
        Vec::new()
    };
    if probe == "P9" {
        libc_size::probe::write_p9_source(&out_dir)?;
    }
    let artifacts = build::build_probe(
        tools,
        &options.root,
        arch,
        set,
        linkage,
        lang,
        probe,
        libc_dir,
        &out_dir,
        &p9_symbols,
    )?;
    let size_output = capture(
        tools,
        &tools.size,
        &[artifacts.stripped.display().to_string()],
        &options.root,
    )?;
    let entry = parse_berkeley(&size_output)?
        .into_iter()
        .next()
        .context("llvm-size returned no row")?;
    let sections_output = capture(
        tools,
        &tools.size,
        &["-A".to_string(), artifacts.stripped.display().to_string()],
        &options.root,
    )?;
    let dynamic_output = capture(
        tools,
        &tools.readobj,
        &[
            "--elf-output-style=GNU".to_string(),
            "-d".to_string(),
            artifacts.stripped.display().to_string(),
        ],
        &options.root,
    )
    .unwrap_or_default();
    let reloc_output = capture(
        tools,
        &tools.readobj,
        &[
            "--elf-output-style=GNU".to_string(),
            "-r".to_string(),
            artifacts.stripped.display().to_string(),
        ],
        &options.root,
    )
    .unwrap_or_default();
    let info = dynamic_info(&dynamic_output, &reloc_output);
    let map_text = std::fs::read_to_string(&artifacts.map).unwrap_or_default();
    let members = parse_map_members(&map_text);
    let demangled = capture(
        tools,
        &tools.nm,
        &[
            "--size-sort".to_string(),
            "--print-size".to_string(),
            "--demangle".to_string(),
            artifacts.binary.display().to_string(),
        ],
        &options.root,
    )
    .unwrap_or_default();
    let raw = capture(
        tools,
        &tools.nm,
        &[
            "--size-sort".to_string(),
            "--print-size".to_string(),
            artifacts.binary.display().to_string(),
        ],
        &options.root,
    )
    .unwrap_or_default();
    let mut symbols: Vec<String> = parse_nm_sizes(&demangled)
        .unwrap_or_default()
        .into_iter()
        .map(|symbol| symbol.name)
        .collect();
    symbols.extend(
        parse_nm_sizes(&raw)
            .unwrap_or_default()
            .into_iter()
            .map(|symbol| symbol.name),
    );
    let top: Vec<String> = parse_nm_sizes(&demangled)
        .unwrap_or_default()
        .into_iter()
        .rev()
        .take(30)
        .map(|symbol| format!("{}={}", symbol.name, symbol.size))
        .collect();
    let row = Row::measured(
        probe,
        arch,
        linkage,
        set,
        lang,
        entry.size.s(),
        entry.size.bss,
        file_size(&artifacts.stripped)?,
        u64::try_from(members.len()).unwrap_or(u64::MAX),
        u64::try_from(info.needed).unwrap_or(u64::MAX),
        u64::try_from(info.jump_slot).unwrap_or(u64::MAX),
        u64::try_from(info.glob_dat).unwrap_or(u64::MAX),
        u64::try_from(info.relative).unwrap_or(u64::MAX),
    );
    Ok((
        row,
        Extra {
            members,
            symbols,
            init_array: libc_size::measure::section_size(
                &parse_sections(&sections_output).unwrap_or_default(),
                ".init_array",
            ),
            top,
        },
    ))
}

fn evaluate_rules(
    file: &rules::RuleFile,
    rows: &[Row],
    extras: &BTreeMap<String, Extra>,
    out: &mut Vec<libc_size::report::RuleResult>,
) {
    for rule in &file.rules {
        for row in rows.iter().filter(|row| row.probe == rule.probe) {
            if !rules::applies(rule, row.linkage, row.set, row.lang) {
                continue;
            }
            let key = rules::cell_key(row.arch, row.linkage, row.set, row.lang);
            let extra = extras.get(&format!("{key}/{}", row.probe));
            let empty_members: Vec<String> = Vec::new();
            let empty_symbols: Vec<String> = Vec::new();
            let (members, symbols, init_array) = match extra {
                Some(extra) => (
                    extra.members.as_slice(),
                    extra.symbols.as_slice(),
                    extra.init_array,
                ),
                None => (empty_members.as_slice(), empty_symbols.as_slice(), 0),
            };
            let p0 = rows.iter().find(|candidate| {
                candidate.probe == "P0"
                    && candidate.arch == row.arch
                    && candidate.linkage == row.linkage
                    && candidate.set == row.set
                    && candidate.lang == row.lang
                    && candidate.is_measured()
            });
            let context = RuleContext {
                s: row.s,
                bss: row.bss,
                init_array,
                members,
                symbols,
                p0_s: p0.map(|candidate| candidate.s),
            };
            out.push(rules::evaluate(rule, row, &context));
        }
    }
}

fn evaluate_large_members(
    tools: &Tools,
    options: &RunOptions,
    set_dirs: &BTreeMap<(Arch, Set), PathBuf>,
    out: &mut Vec<libc_size::report::RuleResult>,
) {
    #[derive(serde::Deserialize, Default)]
    struct AllowFile {
        #[serde(default, rename = "large-member")]
        large_member: Vec<Row>,
    }
    #[derive(serde::Deserialize)]
    struct Row {
        member: String,
    }
    let allow_path = options.root.join("tests/libc/size/size-allowlist.toml");
    let allowed: BTreeSet<String> = std::fs::read_to_string(&allow_path)
        .ok()
        .and_then(|text| toml::from_str::<AllowFile>(&text).ok())
        .map(|file| {
            file.large_member
                .into_iter()
                .map(|row| row.member)
                .collect()
        })
        .unwrap_or_default();
    for ((arch, set), dir) in set_dirs {
        let archive = dir.join("libc.a");
        let size_text = capture(
            tools,
            &tools.size,
            &[archive.display().to_string()],
            &options.root,
        )
        .unwrap_or_default();
        let members = parse_archive_sizes(&size_text).unwrap_or_default();
        let large = libc_size::members::large_members(&members, 4096);
        let unallowed: Vec<String> = large
            .iter()
            .filter(|member| !allowed.contains(&member.name))
            .map(|member| format!("{}={}", member.name, member.size.text))
            .collect();
        out.push(libc_size::report::RuleResult {
            probe: "P0".to_string(),
            arch: *arch,
            linkage: Linkage::Static,
            set: *set,
            lang: Lang::Rust,
            rule: "R-30".to_string(),
            invariant: "R-30".to_string(),
            status: if unallowed.is_empty() { "pass" } else { "fail" }.to_string(),
            detail: if unallowed.is_empty() {
                format!(
                    "{} member(s) above 4096 bytes of .text are allow-listed",
                    large.len()
                )
            } else {
                format!(
                    "{} member(s) above 4096 bytes are not in size-allowlist.toml: {}",
                    unallowed.len(),
                    unallowed.join(", ")
                )
            },
        });
    }
}

fn p9_symbols(root: &Path) -> Result<Vec<String>> {
    let list = root.join("libc/ravenlibc/abi/Symbols.list");
    let mut names = BTreeSet::new();
    if let Ok(text) = std::fs::read_to_string(&list) {
        for line in text.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            names.insert(line.to_string());
        }
    }
    if names.is_empty() {
        let map = root.join("libc/ravenlibc/crates/raven-libc/Symbol.map");
        if let Ok(text) = std::fs::read_to_string(&map) {
            for line in text.lines() {
                let name = line.trim().trim_end_matches(';');
                if !name.is_empty()
                    && !name.starts_with('#')
                    && !name.starts_with('{')
                    && !name.starts_with('}')
                    && !name.starts_with("global:")
                    && !name.starts_with("local:")
                    && !name.contains('*')
                    && !name.ends_with('{')
                {
                    names.insert(name.to_string());
                }
            }
        }
    }
    if names.is_empty() {
        bail!("no symbol list for P9 (abi/Symbols.list and Symbol.map absent)");
    }
    Ok(names.into_iter().collect())
}

fn archive_symbols(tools: &Tools, archive: &Path, root: &Path) -> Result<BTreeSet<String>> {
    if !archive.is_file() {
        bail!("libc.a absent");
    }
    let output = capture(
        tools,
        &tools.nm,
        &[
            "-A".to_string(),
            "-g".to_string(),
            "--defined-only".to_string(),
            archive.display().to_string(),
        ],
        root,
    )?;
    let mut names = BTreeSet::new();
    for member in libc_size::boot::parse_nm_edges(&output) {
        names.extend(member.defined);
    }
    Ok(names)
}

fn compare_sets(
    tools: &Tools,
    root: &Path,
    arch: Arch,
    tiny: Option<&Path>,
    server: Option<&Path>,
) -> SetSymbolRow {
    let (Some(tiny), Some(server)) = (tiny, server) else {
        return SetSymbolRow {
            arch,
            tiny_only: Vec::new(),
            server_only: Vec::new(),
            status: "skipped: no archive".to_string(),
        };
    };
    let public: BTreeSet<String> = match p9_symbols(root) {
        Ok(names) => names.into_iter().collect(),
        Err(error) => {
            return SetSymbolRow {
                arch,
                tiny_only: Vec::new(),
                server_only: Vec::new(),
                status: format!("skipped: {error:#}"),
            };
        }
    };
    let tiny_symbols = archive_symbols(tools, &tiny.join("libc.a"), root);
    let server_symbols = archive_symbols(tools, &server.join("libc.a"), root);
    match (tiny_symbols, server_symbols) {
        (Ok(tiny_symbols), Ok(server_symbols)) => {
            let tiny_public: BTreeSet<String> =
                tiny_symbols.intersection(&public).cloned().collect();
            let server_public: BTreeSet<String> =
                server_symbols.intersection(&public).cloned().collect();
            let tiny_only: Vec<String> = tiny_public.difference(&server_public).cloned().collect();
            let server_only: Vec<String> =
                server_public.difference(&tiny_public).cloned().collect();
            SetSymbolRow {
                arch,
                status: if tiny_only.is_empty() && server_only.is_empty() {
                    "ok".to_string()
                } else {
                    "fail".to_string()
                },
                tiny_only: tiny_only.into_iter().take(64).collect(),
                server_only: server_only.into_iter().take(64).collect(),
            }
        }
        (Err(error), _) | (_, Err(error)) => SetSymbolRow {
            arch,
            tiny_only: Vec::new(),
            server_only: Vec::new(),
            status: format!("skipped: {error:#}"),
        },
    }
}

fn boot_row(
    tools: &Tools,
    options: &RunOptions,
    arch: Arch,
    set_dirs: &BTreeMap<(Arch, Set), PathBuf>,
) -> Option<BootRow> {
    let dir = set_dirs.get(&(arch, Set::Tiny))?;
    let out_dir = options
        .out
        .join(arch.short())
        .join("tiny")
        .join("static")
        .join("rust");
    let p0_binary = out_dir.join("p0.elf");
    let p0_map = out_dir.join("p0.map");
    if !p0_binary.is_file() {
        return Some(BootRow {
            arch,
            syscalls_before_main: 0,
            start_instructions: 0,
            init_array: 0,
            preinit_array: 0,
            ctors: 0,
            early_malloc: Vec::new(),
            forbidden_refs: Vec::new(),
            rodata: Vec::new(),
            notes: Vec::new(),
            status: "skipped: P0 not built".to_string(),
        });
    }
    let p1_binary = out_dir.join("p1.elf");
    let p1_map = out_dir.join("p1.map");
    let inputs = libc_size::boot::Inputs {
        root: &options.root,
        arch,
        p0_binary: &p0_binary,
        p0_map: &p0_map,
        p1_binary: if p1_binary.is_file() {
            Some(&p1_binary)
        } else {
            None
        },
        p1_map: if p1_map.is_file() {
            Some(&p1_map)
        } else {
            None
        },
        archive: &dir.join("libc.a"),
        scratch: &out_dir,
    };
    match libc_size::boot::analyze(tools, &inputs) {
        Ok(row) => Some(row),
        Err(error) => Some(BootRow {
            arch,
            syscalls_before_main: 0,
            start_instructions: 0,
            init_array: 0,
            preinit_array: 0,
            ctors: 0,
            early_malloc: Vec::new(),
            forbidden_refs: Vec::new(),
            rodata: Vec::new(),
            notes: Vec::new(),
            status: format!("skipped: {error:#}"),
        }),
    }
}

fn capture(tools: &Tools, program: &Path, args: &[String], root: &Path) -> Result<String> {
    let _ = tools;
    let spec = CommandSpec::new(program, args.to_vec(), root.to_path_buf());
    let output = spec.run()?;
    if !output.status.success() {
        bail!(
            "{} failed: {}",
            program.display(),
            build::compact_error(&String::from_utf8_lossy(&output.stderr))
        );
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

fn command_check(args: &[String]) -> Result<u8> {
    let root = root_from(args);
    let mut results_file = results_path(&root);
    let mut budgets_file = budgets_path(&root);
    let mut allow_missing = false;
    let mut index = 0usize;
    while index < args.len() {
        match args.get(index).map(String::as_str) {
            Some("--results") => {
                index += 1;
                results_file = PathBuf::from(value(args, index, "--results")?);
            }
            Some("--budgets") => {
                index += 1;
                budgets_file = PathBuf::from(value(args, index, "--budgets")?);
            }
            Some("--allow-missing") => allow_missing = true,
            Some("--root") => index += 1,
            Some(other) => bail!("unknown check flag `{other}`"),
            None => break,
        }
        index += 1;
    }
    let results = Results::read(&results_file)?;
    let budgets = if budgets_file.is_file() {
        Budgets::read(&budgets_file)?
    } else {
        println!(
            "libc-size check: no {}; every measured row has no budget",
            budgets_file.display()
        );
        Budgets::freeze_with(&Results::new(Toolchain::default()), 3)
    };
    let report = check(&results, &budgets, allow_missing);
    print!("{}", report.render());
    Ok(if report.passed() { 0 } else { 1 })
}

fn command_freeze(args: &[String]) -> Result<u8> {
    let root = root_from(args);
    let mut results_file = None;
    let mut out = root.join("tests/libc/size/BUDGETS.toml");
    let mut headroom = 3u64;
    let mut index = 0usize;
    while index < args.len() {
        match args.get(index).map(String::as_str) {
            Some("--results") => {
                index += 1;
                results_file = Some(PathBuf::from(value(args, index, "--results")?));
            }
            Some("--out") => {
                index += 1;
                let value = PathBuf::from(value(args, index, "--out")?);
                out = if value.is_absolute() {
                    value
                } else {
                    root.join(value)
                };
            }
            Some("--headroom-percent") => {
                index += 1;
                headroom = value(args, index, "--headroom-percent")?
                    .parse()
                    .context("--headroom-percent must be an integer")?;
            }
            Some("--root") => index += 1,
            Some(other) => bail!("unknown freeze flag `{other}`"),
            None => break,
        }
        index += 1;
    }
    let Some(results_file) = results_file else {
        bail!("freeze needs --results FILE");
    };
    let results = Results::read(&results_file)?;
    let budgets = Budgets::freeze_with(&results, headroom);
    budgets.write(&out)?;
    println!(
        "libc-size freeze: {} budget rows, {} boot rows, {} libc.so.1 rows, {} known violations -> {}",
        budgets.budget.len(),
        budgets.boot.len(),
        budgets.libc_so.len(),
        budgets.known_violation.len(),
        out.display()
    );
    for known in &budgets.known_violation {
        println!(
            "known-violation: {} {} {} {} {} {} measured={} limit={} ({})",
            known.probe,
            known.arch.short(),
            known.linkage.as_str(),
            known.set.as_str(),
            known.lang.as_str(),
            known.invariant,
            known.measured,
            known.limit,
            known.reason
        );
    }
    Ok(0)
}

fn command_diff(args: &[String]) -> Result<u8> {
    let mut old = None;
    let mut new = None;
    let mut index = 0usize;
    while index < args.len() {
        match args.get(index).map(String::as_str) {
            Some("--old") => {
                index += 1;
                old = Some(PathBuf::from(value(args, index, "--old")?));
            }
            Some("--new") => {
                index += 1;
                new = Some(PathBuf::from(value(args, index, "--new")?));
            }
            Some("--root") => index += 1,
            Some(other) => bail!("unknown diff flag `{other}`"),
            None => break,
        }
        index += 1;
    }
    let (Some(old), Some(new)) = (old, new) else {
        bail!("diff needs --old FILE and --new FILE");
    };
    let old = Results::read(&old)?;
    let new = Results::read(&new)?;
    let mut deltas = Vec::new();
    for row in new.rows.iter().filter(|row| row.is_measured()) {
        let key = format!(
            "{} {} {} {} {}",
            row.probe,
            row.arch.short(),
            row.linkage.as_str(),
            row.set.as_str(),
            row.lang.as_str()
        );
        match old.rows.iter().find(|candidate| {
            candidate.probe == row.probe
                && candidate.arch == row.arch
                && candidate.linkage == row.linkage
                && candidate.set == row.set
                && candidate.lang == row.lang
                && candidate.is_measured()
        }) {
            Some(previous) => {
                let delta = i128::from(row.s) - i128::from(previous.s);
                deltas.push((delta, key, previous.s, row.s));
            }
            None => deltas.push((i128::from(row.s), format!("{key} (new)"), 0, row.s)),
        }
    }
    deltas.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
    println!("libc-size diff (S bytes; positive = grew):");
    for (delta, key, before, after) in deltas.iter().take(50) {
        println!("{delta:+8}  {before} -> {after}  {key}");
    }
    Ok(0)
}

fn command_members(args: &[String]) -> Result<u8> {
    let root = root_from(args);
    let mut map = None;
    let mut index = 0usize;
    while index < args.len() {
        match args.get(index).map(String::as_str) {
            Some("--map") => {
                index += 1;
                map = Some(PathBuf::from(value(args, index, "--map")?));
            }
            Some("--root") => index += 1,
            Some(other) => bail!("unknown members flag `{other}`"),
            None => break,
        }
        index += 1;
    }
    let Some(map) = map else {
        bail!("members needs --map FILE");
    };
    let _ = root;
    let text =
        std::fs::read_to_string(&map).with_context(|| format!("cannot read {}", map.display()))?;
    let members = parse_map_members(&text);
    println!(
        "libc-size members: {} distinct libc.a members",
        members.len()
    );
    for member in &members {
        println!("{member}");
    }
    Ok(0)
}

fn command_boot(args: &[String]) -> Result<u8> {
    let root = root_from(args);
    let mut arch = Arch::X86_64;
    let mut out = root.join("build/size");
    let mut json = false;
    let mut index = 0usize;
    while index < args.len() {
        match args.get(index).map(String::as_str) {
            Some("--arch") => {
                index += 1;
                arch = Arch::parse(value(args, index, "--arch")?)?;
            }
            Some("--out") => {
                index += 1;
                out = PathBuf::from(value(args, index, "--out")?);
                if !out.is_absolute() {
                    out = root.join(out);
                }
            }
            Some("--json") => json = true,
            Some("--libc-dir") => index += 1,
            Some("--root") => index += 1,
            Some(other) => bail!("unknown boot flag `{other}`"),
            None => break,
        }
        index += 1;
    }
    let tools = Tools::discover()?;
    let out_dir = out
        .join(arch.short())
        .join("tiny")
        .join("static")
        .join("rust");
    if !out_dir.join("p0.elf").is_file() {
        let options = RunOptions {
            root: root.clone(),
            arches: vec![arch],
            linkages: vec![Linkage::Static],
            sets: vec![Set::Tiny],
            langs: vec![Lang::Rust],
            probes: vec!["P0", "P1"],
            libc_override: None,
            out: out.clone(),
            no_build_libc: false,
            check_after: false,
        };
        let results = measure(&tools, &options)?;
        results.write(&out.join("libc-size.json"))?;
    }
    let build_dir = root.join(format!("build/{}", arch.short()));
    let set_dir = out.join(arch.short()).join("tiny");
    let archive = set_dir.join("libc.a");
    let p0_binary = out_dir.join("p0.elf");
    let p1_binary = out_dir.join("p1.elf");
    let inputs = libc_size::boot::Inputs {
        root: &root,
        arch,
        p0_binary: &p0_binary,
        p0_map: &out_dir.join("p0.map"),
        p1_binary: if p1_binary.is_file() {
            Some(&p1_binary)
        } else {
            None
        },
        p1_map: if out_dir.join("p1.map").is_file() {
            Some(&out_dir.join("p1.map"))
        } else {
            None
        },
        archive: if archive.is_file() {
            &archive
        } else {
            &build_dir.join("libc.a")
        },
        scratch: &out_dir,
    };
    let row = libc_size::boot::analyze(&tools, &inputs)?;
    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&row).context("serialize boot row")?
        );
    } else {
        println!(
            "libc-size boot: {} syscalls={} instructions={} init_array={} preinit_array={} ctors={} status={}",
            arch.short(),
            row.syscalls_before_main,
            row.start_instructions,
            row.init_array,
            row.preinit_array,
            row.ctors,
            row.status
        );
        for path in &row.early_malloc {
            println!("[boot-malloc] {path}");
        }
        for reference in &row.forbidden_refs {
            println!("[boot-ref] {reference}");
        }
        for string in &row.rodata {
            println!("[boot-rodata] {string}");
        }
        for note in &row.notes {
            println!("{note}");
        }
    }
    // Report command: `check` is the gate. A BP failure is recorded as a known violation by
    // `freeze` and stays visible in the row and the report.
    Ok(0)
}

fn command_relocs(args: &[String]) -> Result<u8> {
    let root = root_from(args);
    let mut so = None;
    let mut index = 0usize;
    while index < args.len() {
        match args.get(index).map(String::as_str) {
            Some("--so") => {
                index += 1;
                so = Some(PathBuf::from(value(args, index, "--so")?));
            }
            Some("--root") => index += 1,
            Some(other) => bail!("unknown relocs flag `{other}`"),
            None => break,
        }
        index += 1;
    }
    let Some(so) = so else {
        bail!("relocs needs --so FILE");
    };
    let arch = soff_arch(&so);
    let relative = so.display().to_string();
    if !so.is_file() {
        println!("{} [{}]", libc_size::relocs::SKIPPED_ABSENT, relative);
        return Ok(0);
    }
    let tools = Tools::discover()?;
    let libc_dir = so.parent().unwrap_or(&root);
    let row = libc_size::relocs::measure(&tools, &root, arch, libc_dir, &relative)?;
    println!("{}", libc_size::relocs::render(&row));
    Ok(if row.jump_slot > 40 { 1 } else { 0 })
}

fn soff_arch(path: &Path) -> Arch {
    let text = path.display().to_string();
    if text.contains("aarch64") {
        Arch::Aarch64
    } else if text.contains("riscv64") {
        Arch::Riscv64
    } else {
        Arch::X86_64
    }
}

fn command_monomorph(args: &[String]) -> Result<u8> {
    let root = root_from(args);
    let mut lib = None;
    let mut allowlist = root.join("tests/libc/size/size-allowlist.toml");
    let mut index = 0usize;
    while index < args.len() {
        match args.get(index).map(String::as_str) {
            Some("--lib") => {
                index += 1;
                lib = Some(PathBuf::from(value(args, index, "--lib")?));
            }
            Some("--allowlist") => {
                index += 1;
                allowlist = PathBuf::from(value(args, index, "--allowlist")?);
            }
            Some("--root") => index += 1,
            Some(other) => bail!("unknown monomorph flag `{other}`"),
            None => break,
        }
        index += 1;
    }
    let Some(lib) = lib else {
        bail!("monomorph needs --lib FILE");
    };
    let tools = Tools::discover()?;
    let text = capture(
        &tools,
        &tools.nm,
        &[
            "--size-sort".to_string(),
            "--print-size".to_string(),
            "--demangle".to_string(),
            lib.display().to_string(),
        ],
        &root,
    )?;
    let groups = libc_size::monomorph::groups(&text);
    let allow = libc_size::monomorph::load_allowlist(&allowlist)?;
    print!("{}", libc_size::monomorph::render(&groups, &allow));
    Ok(
        if libc_size::monomorph::failures(&groups, &allow).is_empty() {
            0
        } else {
            1
        },
    )
}

/// Keep a skip reason free of host paths (result files never carry one) and short.
#[must_use]
pub fn sanitize_reason(text: &str, root: &Path) -> String {
    let mut out = text.to_string();
    if let Ok(absolute) = std::fs::canonicalize(root) {
        out = out.replace(&absolute.display().to_string(), ".");
    }
    if let Ok(current) = std::env::current_dir() {
        out = out.replace(&current.display().to_string(), ".");
    }
    let out = out.lines().next().unwrap_or_default().trim().to_string();
    out.chars().take(160).collect()
}

fn write_text(path: &Path, text: &str) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, text).with_context(|| format!("cannot write {}", path.display()))
}
