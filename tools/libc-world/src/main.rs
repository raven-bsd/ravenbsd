// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `libc-world` command-line interface and the per-package build pipeline.

#![forbid(unsafe_code)]

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use anyhow::{Context, Result, bail};

use libc_world::builders::{self, BuildContext, HostAlias, PlannedStep};
use libc_world::classify::{self, StepLog, SymbolIndex};
use libc_world::extract;
use libc_world::gate;
use libc_world::model::{PackageResult, Step, elf_machine, triple};
use libc_world::patches;
use libc_world::report;
use libc_world::run::{self, RunSpec};
use libc_world::tarball;
use libc_world::world::{Package, World};
use libc_world::wrapper::{self, LinkMode};
use libc_world::{DEFAULT_OUT, DEFAULT_RESULTS, DEFAULT_WORLD};

const USAGE: &str = "\
usage:
  libc-world list    [--world F] [--tier N] [--subset NAME]
  libc-world check   [--world F] [--tarballs DIR] [--strict]
  libc-world build   (<name>... | --subset NAME | --tier N | --all) [--arch x86_64|aarch64|riscv64]
                     [--sysroot DIR] [--tarballs DIR] [--out build/libc-world] [--world F]
                     [--jobs N] [--timeout-secs N] [--link static|dynamic]
                     [--host-alias raven|linux] [--results tools/libc-world/results]
                     [--symbols libc/ravenlibc/abi/census/symbols.toml] [--keep]
  libc-world report  [--results tools/libc-world/results] [--arch A]
  libc-world missing [--results tools/libc-world/results] [--arch A]
                     [--symbols F] [--resources DIR]
  libc-world gate    [--results tools/libc-world/results] [--arch A] [--phase 1|2]
                     [--min-built N] [--world F]";

fn main() -> ExitCode {
    match run() {
        Ok(code) => ExitCode::from(code),
        Err(error) => {
            eprintln!("libc-world: {error:#}");
            ExitCode::from(2)
        }
    }
}

fn run() -> Result<u8> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some(command) = args.first().map(String::as_str) else {
        bail!("missing command\n{USAGE}");
    };
    match command {
        "list" => command_list(&args[1..]),
        "check" => command_check(&args[1..]),
        "build" => command_build(&args[1..]),
        "report" => command_report(&args[1..]),
        "missing" => command_missing(&args[1..]),
        "gate" => command_gate(&args[1..]),
        "-h" | "--help" | "help" => {
            println!("{USAGE}");
            Ok(0)
        }
        other => bail!("unknown command `{other}`\n{USAGE}"),
    }
}

struct ValueParser<'a> {
    args: &'a [String],
    index: usize,
}

impl<'a> ValueParser<'a> {
    fn new(args: &'a [String]) -> Self {
        Self { args, index: 0 }
    }

    fn next(&mut self) -> Option<&'a str> {
        let value = self.args.get(self.index).map(String::as_str);
        if value.is_some() {
            self.index += 1;
        }
        value
    }

    fn value(&mut self, flag: &str) -> Result<&'a str> {
        let Some(value) = self.args.get(self.index) else {
            bail!("{flag} needs a value");
        };
        if value.starts_with("--") {
            bail!("{flag} needs a value");
        }
        self.index += 1;
        Ok(value)
    }
}

fn resolve(root: &Path, value: &str) -> PathBuf {
    let path = Path::new(value);
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        root.join(path)
    }
}

fn command_list(args: &[String]) -> Result<u8> {
    let root = std::env::current_dir()?;
    let mut parser = ValueParser::new(args);
    let mut world_path = root.join(DEFAULT_WORLD);
    let mut tier = None;
    let mut subset = None;
    while let Some(flag) = parser.next() {
        match flag {
            "--world" => world_path = resolve(&root, parser.value("--world")?),
            "--tier" => {
                tier = Some(
                    parser
                        .value("--tier")?
                        .parse::<u8>()
                        .context("--tier needs a number")?,
                )
            }
            "--subset" => subset = Some(parser.value("--subset")?),
            other => bail!("unknown list flag `{other}`\n{USAGE}"),
        }
    }
    let world = World::load(&world_path)?;
    world.validate()?;
    let order = if tier.is_none() && subset.is_none() {
        let all: Vec<&Package> = world.packages().iter().collect();
        world.build_order(&all)?
    } else {
        world.select(&[], tier, subset)?
    };
    for package in order {
        println!("{}", package.name);
    }
    Ok(0)
}

fn command_check(args: &[String]) -> Result<u8> {
    let root = std::env::current_dir()?;
    let mut parser = ValueParser::new(args);
    let mut world_path = root.join(DEFAULT_WORLD);
    let mut tarballs = None;
    let mut strict = false;
    while let Some(flag) = parser.next() {
        match flag {
            "--world" => world_path = resolve(&root, parser.value("--world")?),
            "--tarballs" => tarballs = Some(resolve(&root, parser.value("--tarballs")?)),
            "--strict" => strict = true,
            other => bail!("unknown check flag `{other}`\n{USAGE}"),
        }
    }
    let world = World::load(&world_path)?;
    if let Err(error) = world.validate() {
        eprintln!("libc-world: {error:#}");
        return Ok(1);
    }
    if strict && !world.is_frozen() {
        eprintln!(
            "libc-world: --strict requires freeze = `frozen`; the file is `{}`",
            world.freeze
        );
        return Ok(1);
    }
    if let Some(directory) = tarballs {
        let mut missing = Vec::new();
        for package in world.packages() {
            if package.version == "PLACEHOLDER" {
                continue;
            }
            if !directory.join(&package.tarball).is_file() {
                missing.push(package.name.clone());
            }
        }
        if !missing.is_empty() {
            for name in &missing {
                eprintln!("missing tarball: {name}");
            }
            eprintln!(
                "libc-world: {} tarball(s) absent under {}",
                missing.len(),
                directory.display()
            );
            return Ok(1);
        }
    }
    println!(
        "libc-world: ok ({} packages, {} weighted, freeze {})",
        world.packages().len(),
        world.weighted_total(),
        world.freeze
    );
    Ok(0)
}

struct BuildOptions {
    arch: String,
    sysroot: PathBuf,
    tarballs: PathBuf,
    out: PathBuf,
    results: PathBuf,
    symbols: PathBuf,
    jobs: usize,
    timeout_secs: u64,
    link: LinkMode,
    host_alias: HostAlias,
    keep: bool,
}

fn command_build(args: &[String]) -> Result<u8> {
    let root = std::env::current_dir()?;
    let mut parser = ValueParser::new(args);
    let mut names = Vec::new();
    let mut tier = None;
    let mut subset = None;
    let mut all = false;
    let mut arch = "x86_64".to_string();
    let mut sysroot = None;
    let mut tarballs = None;
    let mut out = root.join(DEFAULT_OUT);
    let mut world_path = root.join(DEFAULT_WORLD);
    let mut results = root.join(DEFAULT_RESULTS);
    let mut symbols = root.join("libc/ravenlibc/abi/census/symbols.toml");
    let mut jobs = 1usize;
    let mut timeout_secs = 3600u64;
    let mut link = LinkMode::Static;
    let mut host_alias = HostAlias::Raven;
    let mut keep = false;
    while let Some(flag) = parser.next() {
        match flag {
            "--all" => all = true,
            "--arch" => arch = parser.value("--arch")?.to_string(),
            "--sysroot" => sysroot = Some(resolve(&root, parser.value("--sysroot")?)),
            "--tarballs" => tarballs = Some(resolve(&root, parser.value("--tarballs")?)),
            "--out" => out = resolve(&root, parser.value("--out")?),
            "--world" => world_path = resolve(&root, parser.value("--world")?),
            "--results" => results = resolve(&root, parser.value("--results")?),
            "--symbols" => symbols = resolve(&root, parser.value("--symbols")?),
            "--jobs" => jobs = parser.value("--jobs")?.parse().context("--jobs")?,
            "--timeout-secs" => {
                timeout_secs = parser
                    .value("--timeout-secs")?
                    .parse()
                    .context("--timeout-secs")?
            }
            "--link" => {
                let value = parser.value("--link")?;
                link = LinkMode::parse(value)
                    .ok_or_else(|| anyhow::anyhow!("unknown link mode `{value}`"))?;
            }
            "--host-alias" => {
                let value = parser.value("--host-alias")?;
                host_alias = HostAlias::parse(value)
                    .ok_or_else(|| anyhow::anyhow!("unknown host alias `{value}`"))?;
            }
            "--keep" => keep = true,
            "--tier" => tier = Some(parser.value("--tier")?.parse::<u8>().context("--tier")?),
            "--subset" => subset = Some(parser.value("--subset")?.to_string()),
            value if value.starts_with('-') => bail!("unknown build flag `{value}`\n{USAGE}"),
            value => names.push(value.to_string()),
        }
    }
    if jobs == 0 {
        bail!("--jobs must be at least 1");
    }
    if elf_machine(&arch).is_none() {
        bail!("unknown architecture `{arch}`");
    }
    let sysroot = sysroot.unwrap_or_else(|| root.join("build").join(&arch).join("sysroot"));
    if !sysroot.is_dir() {
        bail!(
            "missing sysroot {}; run `cargo xtask libc-pack --arch {arch}` first",
            sysroot.display()
        );
    }
    let tarballs = tarballs.unwrap_or_else(|| discover_resources(&root).join("world-src"));
    let world = World::load(&world_path)?;
    world.validate_structure(200, 197.5)?;
    let selection = if all {
        let every: Vec<&Package> = world.packages().iter().collect();
        world.build_order(&every)?
    } else {
        world.select(&names, tier, subset.as_deref())?
    };
    let options = BuildOptions {
        arch,
        sysroot,
        tarballs,
        out,
        results,
        symbols,
        jobs,
        timeout_secs,
        link,
        host_alias,
        keep,
    };
    let tools = Tools::discover();
    let needs_cmake = selection.iter().any(|package| package.build == "cm");
    if let Some(tool) = tools.missing(needs_cmake) {
        eprintln!("skipped: {tool} not found");
        return Ok(77);
    }
    let build_triple = tools.dumpmachine()?;
    run_build(&selection, &options, &tools, &build_triple)?;
    Ok(0)
}

fn command_report(args: &[String]) -> Result<u8> {
    let root = std::env::current_dir()?;
    let mut parser = ValueParser::new(args);
    let mut results = root.join(DEFAULT_RESULTS);
    let mut arch = "x86_64".to_string();
    while let Some(flag) = parser.next() {
        match flag {
            "--results" => results = resolve(&root, parser.value("--results")?),
            "--arch" => arch = parser.value("--arch")?.to_string(),
            "--out" => {
                let _ = parser.value("--out")?;
            }
            other => bail!("unknown report flag `{other}`\n{USAGE}"),
        }
    }
    let world = World::load(&root.join(DEFAULT_WORLD))?;
    let all = report::read_results(&results.join(&arch))?;
    if all.is_empty() {
        bail!("no results under {}", results.join(&arch).display());
    }
    report::write_world(&results, &arch, &all, &world)?;
    let built = all.iter().filter(|result| result.is_built()).count();
    println!(
        "libc-world report: {} package(s), {} built, written to {}",
        all.len(),
        built,
        results.join(&arch).display()
    );
    Ok(0)
}

fn command_missing(args: &[String]) -> Result<u8> {
    let root = std::env::current_dir()?;
    let mut parser = ValueParser::new(args);
    let mut results = root.join(DEFAULT_RESULTS);
    let mut arch = "x86_64".to_string();
    let mut symbols = root.join("libc/ravenlibc/abi/census/symbols.toml");
    let mut resources = discover_resources(&root);
    while let Some(flag) = parser.next() {
        match flag {
            "--results" => results = resolve(&root, parser.value("--results")?),
            "--arch" => arch = parser.value("--arch")?.to_string(),
            "--symbols" => symbols = resolve(&root, parser.value("--symbols")?),
            "--resources" => resources = resolve(&root, parser.value("--resources")?),
            other => bail!("unknown missing flag `{other}`\n{USAGE}"),
        }
    }
    let index = SymbolIndex::load(&symbols)?;
    print!(
        "{}",
        report::missing_backlog(&results, &arch, &index, &resources)?
    );
    Ok(0)
}

fn command_gate(args: &[String]) -> Result<u8> {
    let root = std::env::current_dir()?;
    let mut parser = ValueParser::new(args);
    let mut results = root.join(DEFAULT_RESULTS);
    let mut arch = "x86_64".to_string();
    let mut phase = 1u8;
    let mut min_built = 100u32;
    let mut world_path = root.join(DEFAULT_WORLD);
    while let Some(flag) = parser.next() {
        match flag {
            "--results" => results = resolve(&root, parser.value("--results")?),
            "--arch" => arch = parser.value("--arch")?.to_string(),
            "--phase" => phase = parser.value("--phase")?.parse().context("--phase")?,
            "--min-built" => {
                min_built = parser
                    .value("--min-built")?
                    .parse()
                    .context("--min-built")?
            }
            "--world" => world_path = resolve(&root, parser.value("--world")?),
            other => bail!("unknown gate flag `{other}`\n{USAGE}"),
        }
    }
    if phase != 1 && phase != 2 {
        bail!("--phase must be 1 or 2");
    }
    // A results directory is either the arch directory itself (fixtures) or the root.
    let directory = if results.join(&arch).is_dir()
        && fs::read_dir(results.join(&arch))
            .ok()
            .into_iter()
            .flatten()
            .filter_map(|entry| entry.ok())
            .any(|entry| {
                entry
                    .path()
                    .extension()
                    .is_some_and(|extension| extension == "json")
            }) {
        results.join(&arch)
    } else {
        results
    };
    let world = World::load(&world_path)?;
    let all = report::read_results(&directory)?;
    let report = gate::evaluate(&all, &world, phase, min_built);
    for line in &report.lines {
        println!("{line}");
    }
    Ok(if report.met { 0 } else { 1 })
}

fn discover_resources(root: &Path) -> PathBuf {
    let base = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
    let mut directory = Some(base.as_path());
    while let Some(current) = directory {
        let candidate = current.join("ravenbsd-resources");
        if candidate.is_dir() {
            return candidate;
        }
        directory = current.parent();
    }
    root.parent().unwrap_or(root).join("ravenbsd-resources")
}

/// Host tools resolved once per run.
struct Tools {
    clang: String,
    ld_lld: Option<String>,
    ar: String,
    ranlib: String,
    nm: String,
    strip: String,
    tar: String,
    patch: String,
    make: String,
    host_cc: String,
    cmake: Option<String>,
    ninja: Option<String>,
    path: String,
}

impl Tools {
    fn discover() -> Self {
        let path = std::env::var("PATH").unwrap_or_else(|_| "/usr/bin:/bin".to_string());
        let find = |name: &str| find_tool(&path, name);
        let ar = find("llvm-ar").unwrap_or_else(|| "ar".to_string());
        Self {
            clang: find("clang").unwrap_or_else(|| "clang".to_string()),
            ld_lld: find("ld.lld"),
            ar: ar.clone(),
            ranlib: find("llvm-ranlib").unwrap_or(ar),
            nm: find("llvm-nm").unwrap_or_else(|| "nm".to_string()),
            strip: find("llvm-strip").unwrap_or_else(|| "strip".to_string()),
            tar: find("tar").unwrap_or_else(|| "tar".to_string()),
            patch: find("patch").unwrap_or_else(|| "patch".to_string()),
            make: find("make").unwrap_or_else(|| "make".to_string()),
            host_cc: find("cc").unwrap_or_else(|| "cc".to_string()),
            cmake: find("cmake"),
            ninja: find("ninja"),
            path,
        }
    }

    /// The first required tool that is absent.
    fn missing(&self, needs_cmake: bool) -> Option<String> {
        if self.ld_lld.is_none() {
            return Some("ld.lld".to_string());
        }
        for (name, path) in [
            ("clang", find_tool(&self.path, &self.clang)),
            ("llvm-ar", find_tool(&self.path, &self.ar)),
            ("tar", find_tool(&self.path, &self.tar)),
            ("patch", find_tool(&self.path, &self.patch)),
            ("make", find_tool(&self.path, &self.make)),
        ] {
            if path.is_none() {
                return Some(name.to_string());
            }
        }
        if needs_cmake {
            if self.cmake.is_none() {
                return Some("cmake".to_string());
            }
            if self.ninja.is_none() {
                return Some("ninja".to_string());
            }
        }
        None
    }

    fn dumpmachine(&self) -> Result<String> {
        let output = std::process::Command::new(&self.clang)
            .arg("-dumpmachine")
            .output()
            .with_context(|| format!("cannot run `{} -dumpmachine`", self.clang))?;
        if !output.status.success() {
            bail!("`{} -dumpmachine` failed", self.clang);
        }
        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    }
}

fn find_tool(path: &str, name: &str) -> Option<String> {
    if name.contains('/') && Path::new(name).is_file() {
        return Some(name.to_string());
    }
    for directory in path.split(':') {
        if directory.is_empty() {
            continue;
        }
        let candidate = Path::new(directory).join(name);
        if candidate.is_file() {
            return Some(candidate.display().to_string());
        }
    }
    None
}

fn run_build(
    selection: &[&Package],
    options: &BuildOptions,
    tools: &Tools,
    build_triple: &str,
) -> Result<()> {
    let arch_dir = options.out.join(&options.arch);
    let wrapper = arch_dir.join("bin/raven-cc");
    wrapper::write_wrapper(&wrapper, &options.arch, &options.sysroot, options.link)?;
    let toolchain = arch_dir.join("toolchain.cmake");
    wrapper::write_toolchain_cmake(&toolchain, &options.arch, &wrapper, &options.sysroot)?;
    let index = SymbolIndex::load(&options.symbols)?;
    let ctx = BuildContext {
        arch: options.arch.clone(),
        triple: triple(&options.arch).unwrap_or_default(),
        build_triple: build_triple.to_string(),
        wrapper: wrapper.clone(),
        ar: tools.ar.clone(),
        ranlib: tools.ranlib.clone(),
        make: tools.make.clone(),
        host_cc: tools.host_cc.clone(),
        sysroot: options.sysroot.clone(),
        toolchain_cmake: toolchain,
        host_alias: options.host_alias,
    };
    let selected_names: BTreeSet<&str> = selection
        .iter()
        .map(|package| package.name.as_str())
        .collect();
    let mut results: BTreeMap<String, PackageResult> = BTreeMap::new();
    let mut pending: Vec<&Package> = selection.to_vec();
    while !pending.is_empty() {
        let ready: Vec<usize> = pending
            .iter()
            .enumerate()
            .filter(|(_, package)| {
                package
                    .needs
                    .iter()
                    .filter(|need| selected_names.contains(need.as_str()))
                    .all(|need| results.contains_key(need))
            })
            .map(|(index, _)| index)
            .collect();
        if ready.is_empty() {
            bail!("needs cycle among the selected packages");
        }
        let count = ready.len().min(options.jobs.max(1));
        let chosen: Vec<&Package> = ready[..count].iter().map(|index| pending[*index]).collect();
        let results_ref = &results;
        let ctx_ref = &ctx;
        let index_ref = &index;
        let selected_ref = &selected_names;
        let mut built = std::thread::scope(|scope| {
            let mut handles = Vec::new();
            for package in &chosen {
                handles.push(scope.spawn(move || {
                    let blocked = package.needs.iter().find(|need| {
                        selected_ref.contains(need.as_str())
                            && !results_ref
                                .get(*need)
                                .is_some_and(|result| result.is_built())
                    });
                    if let Some(need) = blocked {
                        return Ok(PackageResult::new(
                            package.n,
                            &package.name,
                            &package.version,
                            &format!("blocked-by:{need}"),
                        ));
                    }
                    build_one(package, options, tools, ctx_ref, index_ref)
                }));
            }
            handles
                .into_iter()
                .map(|handle| handle.join())
                .collect::<Vec<_>>()
        });
        let chosen_names: BTreeSet<String> =
            chosen.iter().map(|package| package.name.clone()).collect();
        drop(chosen);
        pending.retain(|package| !chosen_names.contains(&package.name));
        for joined in built.drain(..) {
            let result = joined.map_err(|_| anyhow::anyhow!("a build worker panicked"))??;
            report::write_result(&options.results, &options.arch, &result)?;
            results.insert(result.name.clone(), result);
        }
    }
    let mut sorted: Vec<&PackageResult> = results.values().collect();
    sorted.sort_by_key(|result| result.n);
    for result in &sorted {
        println!("{}\t{}", result.name, result.status);
    }
    let built = sorted.iter().filter(|result| result.is_built()).count();
    println!(
        "libc-world: {} package(s), {} built, {} not built",
        sorted.len(),
        built,
        sorted.len() - built
    );
    Ok(())
}

fn build_one(
    package: &Package,
    options: &BuildOptions,
    tools: &Tools,
    ctx: &BuildContext,
    index: &SymbolIndex,
) -> Result<PackageResult> {
    let mut result = PackageResult::new(package.n, &package.name, &package.version, "placeholder");
    result.host_alias = options.host_alias.as_str().to_string();
    result.link = options.link.as_str().to_string();
    if package.version == "PLACEHOLDER" {
        result.failure = Some(libc_world::model::Failure {
            class: "placeholder".to_string(),
            detail: "version not frozen (H-12)".to_string(),
        });
        return Ok(result);
    }
    let package_dir = options.out.join(&options.arch).join(&package.name);
    let src = package_dir.join("src");
    let build = package_dir.join("build");
    let dest = package_dir.join("dest");
    let log = package_dir.join("log");
    if package_dir.exists() {
        fs::remove_dir_all(&package_dir)?;
    }
    fs::create_dir_all(&src)?;
    fs::create_dir_all(&dest)?;
    fs::create_dir_all(&log)?;

    let tarball_path = options.tarballs.join(&package.tarball);
    if !tarball_path.is_file() {
        result.status = "tarball-missing".to_string();
        result.failure = Some(libc_world::model::Failure {
            class: "tarball-missing".to_string(),
            detail: "owner has not supplied it (H-11)".to_string(),
        });
        return Ok(result);
    }
    let digest = tarball::file_sha256(&tarball_path)?;
    if !package.sha256.eq_ignore_ascii_case(&digest) {
        result.status = "tarball-bad-sha".to_string();
        result.failure = Some(libc_world::model::Failure {
            class: "tarball-bad-sha".to_string(),
            detail: format!("expected {}, got {digest}", package.sha256),
        });
        return Ok(result);
    }
    let entries = tarball::list(&tools.tar, &tarball_path)?;
    if let Err(problem) = tarball::check(&entries) {
        result.status = problem.status().to_string();
        result.failure = Some(libc_world::model::Failure {
            class: problem.status().to_string(),
            detail: problem.reason().to_string(),
        });
        return Ok(result);
    }
    tarball::extract(&tools.tar, &tarball_path, &src)?;

    let patch_dir = std::path::Path::new("tools/libc-world/patches").join(&package.name);
    let applied = patches::apply_all(&tools.patch, &patch_dir, &src)?;
    let counts = patches::account(&applied);
    result.libc_patches = counts.libc;
    result.platform_patches = counts.platform;
    result.toolchain_patches = counts.toolchain;
    for patch in &applied {
        result.patches.push(libc_world::model::PatchRecord {
            file: patch.file.clone(),
            class: patch.header.class.as_str().to_string(),
            reason: patch.header.reason.clone(),
            reclassified_as_libc: patch.reclassified_as_libc,
        });
        if patch.reclassified_as_libc {
            eprintln!("reclassified-as-libc: {}", patch.file);
        }
    }
    result.configure_flags_disabled_because_libc = package
        .flag_notes
        .iter()
        .filter(|(_, note)| note.starts_with("libc:"))
        .map(|(flag, _)| flag.clone())
        .collect();

    let steps = match builders::plan(&package.build, package, ctx, &src, &build, &dest) {
        Ok(steps) => steps,
        Err(error) => {
            result.status = "skipped".to_string();
            result.failure = Some(libc_world::model::Failure {
                class: "skipped".to_string(),
                detail: error.to_string(),
            });
            return Ok(result);
        }
    };
    if package.build == "kc" && !package.config_lines.is_empty() {
        builders::apply_config_lines(&src, &package.config_lines)?;
    }
    let env = step_environment(package, options, tools, ctx, &log, &dest);
    let mut logs = Vec::new();
    let mut timed_out = false;
    for planned in &steps {
        if planned.name == "oldconfig" {
            builders::apply_config_lines(&src, &package.config_lines)?;
        }
        let output = execute_step(planned, &env, options.timeout_secs, &log)?;
        logs.push(StepLog {
            name: planned.name.clone(),
            exit: output.exit,
            text: output.text.clone(),
        });
        result.steps.push(Step {
            name: planned.name.clone(),
            exit: output.exit,
        });
        if output.timed_out {
            timed_out = true;
            break;
        }
        if !matches!(output.exit, Some(0)) {
            break;
        }
    }
    let extraction = extract::scan(
        &tools.nm,
        &[src.clone(), build.clone(), dest.clone()],
        &options.arch,
        &options.sysroot,
        &log,
    )?;
    result.undefined_symbols = u32::try_from(extraction.undefined.len()).unwrap_or(u32::MAX);
    result.headers_used = u32::try_from(extraction.headers.len()).unwrap_or(u32::MAX);
    result.libs = extraction.libs.iter().cloned().collect();
    libc_world::census_out::write_undef(
        &options.results,
        &options.arch,
        &package.name,
        &extraction.undefined,
    )?;
    report::write_headers(
        &options.results,
        &options.arch,
        &package.name,
        &extraction.headers,
    )?;
    result.size_bytes = directory_size(&dest)?;

    let replacements = vec![
        (src.clone(), "<src>"),
        (options.sysroot.clone(), "<sysroot>"),
        (options.out.clone(), "<out>"),
    ];
    let classified = classify::classify(&logs, index, &replacements);
    if timed_out {
        result.status = "timeout".to_string();
        result.failure = Some(libc_world::model::Failure {
            class: "timeout".to_string(),
            detail: "step exceeded --timeout-secs".to_string(),
        });
    } else if logs.iter().any(|log| !matches!(log.exit, Some(0))) {
        result.status = "failed".to_string();
        if let Some(classified) = classified {
            result.findings = classified.findings;
            result.failure = Some(libc_world::model::Failure {
                class: classified.class,
                detail: classified.detail,
            });
            result.implicit_decls = classified.implicit_decls.into_iter().collect();
            result.missing_headers = classified.missing_headers.into_iter().collect();
            result.missing_libs = classified.missing_libs.into_iter().collect();
            result.libc_probe_failed = classified.libc_probe_failed.into_iter().collect();
        } else {
            result.failure = Some(libc_world::model::Failure {
                class: "other".to_string(),
                detail: "step failed without a classifier match".to_string(),
            });
        }
    } else {
        result.status = "built".to_string();
        if let Some(classified) = classified {
            result.findings = classified.findings;
            result.libc_probe_failed = classified.libc_probe_failed.into_iter().collect();
            result.implicit_decls = classified.implicit_decls.into_iter().collect();
            result.missing_headers = classified.missing_headers.into_iter().collect();
            result.missing_libs = classified.missing_libs.into_iter().collect();
        }
    }
    if !options.keep {
        let _ = fs::remove_dir_all(&src);
        let _ = fs::remove_dir_all(&build);
    }
    Ok(result)
}

fn step_environment(
    package: &Package,
    options: &BuildOptions,
    tools: &Tools,
    ctx: &BuildContext,
    log: &Path,
    dest: &Path,
) -> Vec<(String, String)> {
    vec![
        ("PATH".to_string(), tools.path.clone()),
        ("CC".to_string(), ctx.wrapper.display().to_string()),
        ("AR".to_string(), tools.ar.clone()),
        ("RANLIB".to_string(), tools.ranlib.clone()),
        ("NM".to_string(), tools.nm.clone()),
        ("STRIP".to_string(), tools.strip.clone()),
        ("RAVEN_WORLD_LOG".to_string(), log.display().to_string()),
        ("CFLAGS".to_string(), String::new()),
        ("LDFLAGS".to_string(), String::new()),
        (
            "PKG_CONFIG_LIBDIR".to_string(),
            options
                .sysroot
                .join("usr/lib/pkgconfig")
                .display()
                .to_string(),
        ),
        (
            "PKG_CONFIG_SYSROOT_DIR".to_string(),
            options.sysroot.display().to_string(),
        ),
        (
            "HOME".to_string(),
            options
                .out
                .join(&options.arch)
                .join(&package.name)
                .join("home")
                .display()
                .to_string(),
        ),
        ("TZ".to_string(), "UTC".to_string()),
        ("LC_ALL".to_string(), "C".to_string()),
        ("DESTDIR".to_string(), dest.display().to_string()),
    ]
}

fn execute_step(
    planned: &PlannedStep,
    env: &[(String, String)],
    timeout_secs: u64,
    log: &Path,
) -> Result<run::RunOutput> {
    let mut spec = RunSpec::new(
        planned.program.clone(),
        planned.args.clone(),
        planned.cwd.clone(),
        log.join(format!("{}.log", planned.name)),
        timeout_secs,
    );
    spec.env = env.to_vec();
    for (key, value) in &planned.env {
        spec = spec.with_env(key.clone(), value.clone());
    }
    run::run(&spec)
}

fn directory_size(directory: &Path) -> Result<u64> {
    let mut total = 0u64;
    if !directory.is_dir() {
        return Ok(0);
    }
    for entry in fs::read_dir(directory)? {
        let entry = entry?;
        let metadata = entry.metadata()?;
        if metadata.is_dir() {
            total += directory_size(&entry.path())?;
        } else if metadata.is_file() {
            total += metadata.len();
        }
    }
    Ok(total)
}

#[cfg(test)]
#[path = "main_ut.rs"]
mod tests;
