// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Donor build driver (ticket C35, section 4.4).
//!
//! Every source is compiled with the system C compiler, the objects are merged with `ld -r`, the
//! defined globals are renamed with `objcopy --redefine-syms`, and the result is linked into
//! `liboracle_<family>.so`. A recipe's `exports` must equal the merged object's defined globals and
//! the dynamic symbol table must contain exactly `oracle_<export>`.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, bail};

use crate::failure::Failure;
use crate::gate;
use crate::pin;
use crate::recipe::{Defaults, Recipe, Source};
use crate::tools::Tools;

/// Everything one build needs.
pub(crate) struct BuildRequest<'a> {
    /// Recipe to build.
    pub(crate) recipe: &'a Recipe,
    /// Shared defines from `defaults.toml`.
    pub(crate) defaults: &'a Defaults,
    /// Recipes directory (its parent holds `shim/`).
    pub(crate) recipes: &'a Path,
    /// Donor checkout root.
    pub(crate) resources: &'a Path,
    /// Output root (`build/libc-oracle` by default).
    pub(crate) out: &'a Path,
    /// Optimisation levels, in order.
    pub(crate) opt: Vec<String>,
    /// Add UBSan flags to compile and link.
    pub(crate) sanitize: bool,
    /// Skip the donor pin check.
    pub(crate) no_pin_check: bool,
    /// Ad-hoc one-file family: exports are taken from `nm`, not from a recipe.
    pub(crate) adhoc: bool,
}

/// Build every requested optimisation level; returns the output directories.
pub(crate) fn build(request: &BuildRequest<'_>) -> Result<Vec<PathBuf>> {
    let tools = crate::tools::discover()?;
    let lock = pin::read_lock(request.resources).ok();
    let mut directories = Vec::new();
    let mut export_sets: Vec<BTreeSet<String>> = Vec::new();
    for opt in &request.opt {
        let (directory, exports) = build_one(request, &tools, opt, lock.as_ref())?;
        directories.push(directory);
        export_sets.push(exports);
    }
    if let Some(first) = export_sets.first() {
        for (index, set) in export_sets.iter().enumerate().skip(1) {
            if set != first {
                return Err(Failure::refused(format!(
                    "optimisation levels {} and {} export different symbols",
                    request.opt[0], request.opt[index]
                )));
            }
        }
    }
    Ok(directories)
}

fn build_one(
    request: &BuildRequest<'_>,
    tools: &Tools,
    opt: &str,
    lock: Option<&BTreeMap<String, pin::LockEntry>>,
) -> Result<(PathBuf, BTreeSet<String>)> {
    let directory = request.out.join(&request.recipe.family).join(opt);
    if directory.exists() {
        fs::remove_dir_all(&directory)
            .with_context(|| format!("cannot clean {}", directory.display()))?;
    }
    fs::create_dir_all(directory.join("obj"))
        .with_context(|| format!("cannot create {}", directory.display()))?;

    let oracle_root = request.recipes.parent().unwrap_or_else(|| Path::new("."));
    let shim_root = oracle_root.join("shim");
    let includes = resolve_includes(request, &shim_root)?;
    let mut recorded_commands: Vec<Vec<String>> = Vec::new();
    let mut object_files = Vec::new();
    let mut sources = Vec::new();

    for source in &request.recipe.source {
        let source_path = request.resources.join(&source.donor).join(&source.path);
        if !source_path.is_file() {
            bail!(
                "recipe {}: donor file {} is missing",
                request.recipe.family,
                source_path.display()
            );
        }
        if !request.no_pin_check {
            pin::check(request.resources, &source.donor, &source.path)?;
        }
        gate::check_file(&source_path, &source.donor, request.resources)?;
        let stem = Path::new(&source.path)
            .file_stem()
            .map(|stem| stem.to_string_lossy().into_owned())
            .unwrap_or_default();
        let object = format!("obj/{stem}.o");
        let (actual, recorded) =
            compile_args(opt, request, source, &includes, &source_path, &object);
        run_tool(&tools.cc, &actual, &directory)?;
        recorded_commands.push(prefix(&tools.cc, &recorded));
        object_files.push(object);
        sources.push(format!("{}:{}", source.donor, source.path));
    }

    let mut ld_args = vec!["-r".to_string(), "-o".to_string(), "merged.o".to_string()];
    ld_args.extend(object_files.iter().cloned());
    run_tool(&tools.ld, &ld_args, &directory)?;
    recorded_commands.push(prefix(&tools.ld, &ld_args));

    let nm_args = vec![
        "-g".to_string(),
        "--defined-only".to_string(),
        "merged.o".to_string(),
    ];
    let merged = capture(&tools.nm, &nm_args, &directory)?;
    recorded_commands.push(prefix(&tools.nm, &nm_args));
    let defined = parse_nm(&merged)?;
    if defined.is_empty() && request.recipe.kind != "header" {
        bail!(
            "recipe {}: merged object defines no globals",
            request.recipe.family
        );
    }

    if request.adhoc {
        // The export list is whatever the single file defines.
    } else {
        let expected: Vec<String> = request.recipe.exports.clone();
        if defined != expected {
            let missing: Vec<&String> = expected
                .iter()
                .filter(|name| !defined.contains(name))
                .collect();
            let extra: Vec<&String> = defined
                .iter()
                .filter(|name| !expected.contains(name))
                .collect();
            return Err(Failure::refused(format!(
                "exports mismatch for {}: missing {missing:?}, extra {extra:?}",
                request.recipe.family
            )));
        }
    }

    let redef: String = defined
        .iter()
        .map(|name| format!("{name} oracle_{name}\n"))
        .collect();
    fs::write(directory.join("redef.txt"), redef)
        .with_context(|| format!("cannot write {}", directory.join("redef.txt").display()))?;
    let objcopy_args = vec![
        "--redefine-syms=redef.txt".to_string(),
        "merged.o".to_string(),
        "merged_pfx.o".to_string(),
    ];
    run_tool(&tools.objcopy, &objcopy_args, &directory)?;
    recorded_commands.push(prefix(&tools.objcopy, &objcopy_args));

    let library = format!("liboracle_{}.so", request.recipe.family.replace('-', "_"));
    let mut shared_args = Vec::new();
    if request.sanitize {
        shared_args.push("-fsanitize=undefined".to_string());
    }
    shared_args.extend([
        "-shared".to_string(),
        "-o".to_string(),
        library.clone(),
        "merged_pfx.o".to_string(),
    ]);
    run_tool(&tools.cc, &shared_args, &directory)?;
    recorded_commands.push(prefix(&tools.cc, &shared_args));

    let dynamic_args = vec![
        "-D".to_string(),
        "--defined-only".to_string(),
        library.clone(),
    ];
    let dynamic = capture(&tools.nm, &dynamic_args, &directory)?;
    recorded_commands.push(prefix(&tools.nm, &dynamic_args));
    let exported = parse_nm(&dynamic)?;
    let mut wanted: Vec<String> = defined
        .iter()
        .map(|name| format!("oracle_{name}"))
        .collect();
    wanted.sort();
    if exported != wanted {
        let missing: Vec<&String> = wanted
            .iter()
            .filter(|name| !exported.contains(name))
            .collect();
        let extra: Vec<&String> = exported
            .iter()
            .filter(|name| !wanted.contains(name))
            .collect();
        return Err(Failure::refused(format!(
            "dynamic exports of {} do not match the definitions: missing {missing:?}, extra {extra:?}",
            library
        )));
    }

    let exports_file = wanted.join("\n");
    fs::write(directory.join("exports.txt"), format!("{exports_file}\n"))
        .with_context(|| format!("cannot write {}", directory.join("exports.txt").display()))?;
    let donors = donors_of(request.recipe, lock);
    write_build_json(
        &directory.join("build.json"),
        request.recipe,
        opt,
        &donors,
        &recorded_commands,
        &sources,
    )?;

    for line in dynamic.lines() {
        println!("{line}");
    }
    println!("RAVEN_ORACLE_DIR={}", directory.display());
    Ok((directory, defined.into_iter().collect()))
}

fn compile_args(
    opt: &str,
    request: &BuildRequest<'_>,
    source: &Source,
    includes: &[(String, String)],
    source_path: &Path,
    object: &str,
) -> (Vec<String>, Vec<String>) {
    let mut actual = Vec::new();
    let mut recorded = Vec::new();
    for flag in ["-fno-builtin", "-fPIC", "-w"] {
        actual.push(flag.to_string());
        recorded.push(flag.to_string());
    }
    if !request
        .recipe
        .flags
        .iter()
        .any(|flag| flag.starts_with("-std="))
    {
        actual.push("-std=gnu11".to_string());
        recorded.push("-std=gnu11".to_string());
    }
    actual.push(format!("-{opt}"));
    recorded.push(format!("-{opt}"));
    if request.sanitize {
        for flag in ["-fsanitize=undefined", "-fno-sanitize-recover=undefined"] {
            actual.push(flag.to_string());
            recorded.push(flag.to_string());
        }
    }
    for define in request
        .defaults
        .defines
        .iter()
        .chain(request.recipe.defines.iter())
    {
        actual.push(format!("-D{define}"));
        recorded.push(format!("-D{define}"));
    }
    for (display, path) in includes {
        actual.push(format!("-I{path}"));
        recorded.push(format!("-I{display}"));
    }
    for flag in &request.recipe.flags {
        actual.push(flag.clone());
        recorded.push(flag.clone());
    }
    actual.push("-c".to_string());
    recorded.push("-c".to_string());
    if request.recipe.kind == "header" {
        actual.push("-x".to_string());
        actual.push("c".to_string());
        recorded.push("-x".to_string());
        recorded.push("c".to_string());
    }
    actual.push(source_path.display().to_string());
    recorded.push(format!("{}:{}", source.donor, source.path));
    actual.push("-o".to_string());
    recorded.push("-o".to_string());
    actual.push(object.to_string());
    recorded.push(object.to_string());
    (actual, recorded)
}

fn resolve_includes(request: &BuildRequest<'_>, shim_root: &Path) -> Result<Vec<(String, String)>> {
    let mut resolved = Vec::new();
    for include in &request.recipe.include {
        if let Some(rest) = include.strip_prefix("donor:") {
            let Some((donor, dir)) = rest.split_once('/') else {
                bail!("include `{include}` is malformed");
            };
            let path = request.resources.join(donor).join(dir);
            if !path.is_dir() {
                bail!("include directory {} is missing", path.display());
            }
            let path = std::path::absolute(&path)
                .with_context(|| format!("cannot make {} absolute", path.display()))?;
            resolved.push((include.clone(), path.display().to_string()));
        } else if let Some(dir) = include.strip_prefix("shim:") {
            let path = shim_root.join(dir);
            if !path.is_dir() {
                bail!("shim directory {} is missing", path.display());
            }
            let path = std::path::absolute(&path)
                .with_context(|| format!("cannot make {} absolute", path.display()))?;
            resolved.push((include.clone(), path.display().to_string()));
        } else {
            bail!("include `{include}` is malformed");
        }
    }
    Ok(resolved)
}

fn donors_of(
    recipe: &Recipe,
    lock: Option<&BTreeMap<String, pin::LockEntry>>,
) -> BTreeMap<String, String> {
    let mut donors = BTreeMap::new();
    for source in &recipe.source {
        if let Some(lock) = lock
            && let Some(entry) = lock.get(&source.donor)
        {
            donors.insert(source.donor.clone(), entry.sha.clone());
        }
    }
    donors
}

fn prefix(program: &str, args: &[String]) -> Vec<String> {
    let mut recorded = vec![program.to_string()];
    recorded.extend(args.iter().cloned());
    recorded
}

fn run_tool(program: &str, args: &[String], directory: &Path) -> Result<()> {
    let status = Command::new(program)
        .args(args)
        .current_dir(directory)
        .status()
        .with_context(|| format!("cannot run `{program}`"))?;
    if !status.success() {
        bail!(
            "`{program}` failed with exit {} in {}",
            status
                .code()
                .map_or_else(|| "signal".to_string(), |code| code.to_string()),
            directory.display()
        );
    }
    Ok(())
}

fn capture(program: &str, args: &[String], directory: &Path) -> Result<String> {
    let output = Command::new(program)
        .args(args)
        .current_dir(directory)
        .output()
        .with_context(|| format!("cannot run `{program}`"))?;
    if !output.status.success() {
        bail!(
            "`{program}` failed with exit {}: {}",
            output.status.code().unwrap_or(1),
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

/// Parse `nm` output; accepted types are `T D B R W V i`, `U` is ignored.
pub(crate) fn parse_nm(text: &str) -> Result<Vec<String>> {
    let mut names = Vec::new();
    for line in text.lines() {
        let fields: Vec<&str> = line.split_whitespace().collect();
        if fields.is_empty() {
            continue;
        }
        let (kind, name) = match fields.len() {
            0 => continue,
            1 => bail!("cannot parse nm line `{line}`"),
            2 => (fields[0], fields[1]),
            _ => (fields[fields.len() - 2], fields[fields.len() - 1]),
        };
        if kind.len() != 1 {
            bail!("cannot parse nm line `{line}`");
        }
        if matches!(kind, "T" | "D" | "B" | "R" | "W" | "V" | "i") {
            names.push(name.to_string());
        }
    }
    names.sort();
    names.dedup();
    Ok(names)
}

fn write_build_json(
    path: &Path,
    recipe: &Recipe,
    opt: &str,
    donors: &BTreeMap<String, String>,
    commands: &[Vec<String>],
    sources: &[String],
) -> Result<()> {
    let mut text = String::new();
    text.push('{');
    text.push_str(&format!("\"family\":{}", json_string(&recipe.family)));
    text.push_str(&format!(",\"opt\":{}", json_string(opt)));
    text.push_str(",\"donors\":{");
    for (index, (name, sha)) in donors.iter().enumerate() {
        if index > 0 {
            text.push(',');
        }
        text.push_str(&json_string(name));
        text.push(':');
        text.push_str(&json_string(sha));
    }
    text.push('}');
    text.push_str(",\"commands\":[");
    for (index, command) in commands.iter().enumerate() {
        if index > 0 {
            text.push(',');
        }
        text.push('[');
        for (arg_index, arg) in command.iter().enumerate() {
            if arg_index > 0 {
                text.push(',');
            }
            text.push_str(&json_string(arg));
        }
        text.push(']');
    }
    text.push(']');
    text.push_str(",\"sources\":[");
    for (index, source) in sources.iter().enumerate() {
        if index > 0 {
            text.push(',');
        }
        text.push_str(&json_string(source));
    }
    text.push(']');
    text.push('}');
    text.push('\n');
    fs::write(path, text).with_context(|| format!("cannot write {}", path.display()))
}

fn json_string(value: &str) -> String {
    let mut out = String::with_capacity(value.len() + 2);
    out.push('"');
    for ch in value.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if c.is_control() => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

#[cfg(test)]
#[path = "build_ut.rs"]
mod tests;
