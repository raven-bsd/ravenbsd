// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `libc-oracle`: build donor C originals into renamed shared objects and vendor donor test C.
//!
//! The tool is host-only and uses the system C compiler, linker, `nm` and `objcopy` through
//! [`std::process::Command`] with argument vectors. It never compiles C through a shell and never
//! links a donor object into a shipped artifact.
//!
//! Exit codes: 0 success, 1 a gate refusal (licence, donor pin, exports), 2 usage/I/O/recipe error,
//! 77 a required host tool is missing (reported as `skipped`). Non-Linux hosts are skipped (77).

#![forbid(unsafe_code)]

mod build;
mod failure;
mod gate;
mod pin;
mod recipe;
mod tools;
mod vendor;

#[cfg(test)]
mod testutil;

pub use failure::exit_code;

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};

use crate::failure::Failure;
use crate::recipe::{Defaults, Recipe, Source};

const USAGE: &str = "\
usage:
  libc-oracle build  (<family> | --all) [--opt O0,O2] [--sanitize] [--out DIR]
                     [--recipes DIR] [--resources DIR] [--no-pin-check]
  libc-oracle adhoc  <donor> <file> [--type c|header] [--opt O2] [--out DIR]
                     [--recipes DIR] [--resources DIR] [--no-pin-check]
  libc-oracle list   [--recipes DIR]
  libc-oracle check  [--recipes DIR] [--resources DIR]
  libc-oracle vendor --donor D --from DIR --to DIR [--data] [--dry-run]
                     [--date YYYY-MM-DD] [--resources DIR] [--no-pin-check]

exit: 0 ok, 1 gate refused, 2 usage/I/O/recipe error, 77 required tool missing";

/// Run the command line; returns the process exit code.
///
/// The caller prints any error and calls [`exit_code`] to pick the process status.
pub fn run(args: &[String]) -> Result<u8> {
    if !cfg!(target_os = "linux") {
        return Err(Failure::skipped_message("unsupported host"));
    }
    let Some(command) = args.first() else {
        bail!("missing subcommand\n{USAGE}");
    };
    match command.as_str() {
        "build" => command_build(&args[1..]),
        "adhoc" => command_adhoc(&args[1..]),
        "list" => command_list(&args[1..]),
        "check" => command_check(&args[1..]),
        "vendor" => command_vendor(&args[1..]),
        "-h" | "--help" | "help" => {
            println!("{USAGE}");
            Ok(0)
        }
        other => bail!("unknown subcommand `{other}`\n{USAGE}"),
    }
}

fn default_recipes() -> PathBuf {
    PathBuf::from("tests/libc/oracle/recipes")
}

fn default_out() -> PathBuf {
    PathBuf::from("build/libc-oracle")
}

fn command_build(args: &[String]) -> Result<u8> {
    let mut family = None;
    let mut all = false;
    let mut opt = None;
    let mut sanitize = false;
    let mut no_pin_check = false;
    let mut recipes = default_recipes();
    let mut out = default_out();
    let mut resources = None;
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--all" => all = true,
            "--sanitize" => sanitize = true,
            "--no-pin-check" => no_pin_check = true,
            "--opt" => opt = Some(recipe::parse_opt_list(&value(args, &mut index, "--opt")?)?),
            "--out" => out = PathBuf::from(value(args, &mut index, "--out")?),
            "--recipes" => recipes = PathBuf::from(value(args, &mut index, "--recipes")?),
            "--resources" => {
                resources = Some(PathBuf::from(value(args, &mut index, "--resources")?))
            }
            flag if flag.starts_with('-') => bail!("unknown build flag `{flag}`"),
            value => {
                if family.replace(value.to_string()).is_some() {
                    bail!("build takes one family, not several");
                }
            }
        }
        index += 1;
    }
    if all && family.is_some() {
        bail!("build takes a family or --all, not both");
    }
    if !all && family.is_none() {
        bail!("build needs a family or --all");
    }
    let resources = resolve_resources(resources)?;
    let defaults = recipe::read_defaults(&recipes)?;
    let known = known_donors(&resources);
    let loaded = if let Some(family) = family {
        vec![recipe::load_family(&recipes, &family, known.as_ref())?]
    } else {
        let recipes = recipe::load_all(&recipes, known.as_ref())?;
        if recipes.is_empty() {
            bail!("no recipes found");
        }
        recipes
    };
    for recipe in &loaded {
        if recipe.model {
            println!("{}: model-only, no C build", recipe.family);
            continue;
        }
        let levels = opt.clone().unwrap_or_else(|| recipe.opt.clone());
        build_recipe(
            recipe,
            &defaults,
            &recipes,
            &resources,
            &out,
            levels,
            sanitize,
            no_pin_check,
            false,
        )?;
    }
    Ok(0)
}

fn command_adhoc(args: &[String]) -> Result<u8> {
    let mut positional = Vec::new();
    let mut opt: Option<Vec<String>> = None;
    let mut kind = "c".to_string();
    let mut no_pin_check = false;
    let mut recipes = default_recipes();
    let mut out = default_out();
    let mut resources = None;
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--opt" => opt = Some(recipe::parse_opt_list(&value(args, &mut index, "--opt")?)?),
            "--out" => out = PathBuf::from(value(args, &mut index, "--out")?),
            "--recipes" => recipes = PathBuf::from(value(args, &mut index, "--recipes")?),
            "--resources" => {
                resources = Some(PathBuf::from(value(args, &mut index, "--resources")?))
            }
            "--no-pin-check" => no_pin_check = true,
            "--type" => kind = value(args, &mut index, "--type")?,
            flag if flag.starts_with('-') => bail!("unknown adhoc flag `{flag}`"),
            value => positional.push(value.to_string()),
        }
        index += 1;
    }
    if positional.len() != 2 {
        bail!("adhoc needs <donor> <file>\n{USAGE}");
    }
    let donor = positional[0].clone();
    let file = positional[1].clone();
    if kind != "c" && kind != "header" {
        bail!("adhoc --type `{kind}` must be `c` or `header`");
    }
    if !valid_adhoc_path(&file, &kind) {
        let extension = if kind == "header" { ".h" } else { ".c" };
        bail!("adhoc file `{file}` must be relative, without `..`, and end in {extension}");
    }
    let stem = Path::new(&file)
        .file_stem()
        .map(|stem| stem.to_string_lossy().into_owned())
        .unwrap_or_default();
    let family = format!("adhoc-{stem}");
    if !family
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        bail!("cannot derive a family name from `{file}`");
    }
    let recipe = Recipe {
        schema: 1,
        family,
        description: format!("ad-hoc build of {donor}:{file}"),
        kind,
        model: false,
        exports: Vec::new(),
        opt: vec!["O2".to_string()],
        defines: Vec::new(),
        include: Vec::new(),
        flags: Vec::new(),
        source: vec![Source { donor, path: file }],
    };
    let resources = resolve_resources(resources)?;
    let defaults = recipe::read_defaults(&recipes)?;
    build_recipe(
        &recipe,
        &defaults,
        &recipes,
        &resources,
        &out,
        opt.unwrap_or_else(|| recipe.opt.clone()),
        false,
        no_pin_check,
        true,
    )?;
    Ok(0)
}

fn command_list(args: &[String]) -> Result<u8> {
    let mut recipes = default_recipes();
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--recipes" => recipes = PathBuf::from(value(args, &mut index, "--recipes")?),
            flag => bail!("unknown list flag `{flag}`"),
        }
        index += 1;
    }
    for recipe in recipe::load_all(&recipes, None)? {
        println!("{}", recipe.family);
    }
    Ok(0)
}

fn command_check(args: &[String]) -> Result<u8> {
    let mut recipes_dir = default_recipes();
    let mut resources = None;
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--recipes" => recipes_dir = PathBuf::from(value(args, &mut index, "--recipes")?),
            "--resources" => {
                resources = Some(PathBuf::from(value(args, &mut index, "--resources")?))
            }
            flag => bail!("unknown check flag `{flag}`"),
        }
        index += 1;
    }
    let resources = resolve_resources(resources)?;
    let known = known_donors(&resources);
    let loaded = recipe::load_all(&recipes_dir, known.as_ref())?;
    if loaded.is_empty() {
        bail!("no recipes found under {}", recipes_dir.display());
    }
    let mut checked = 0usize;
    let mut skipped = 0usize;
    let mut missing = BTreeSet::new();
    for recipe in &loaded {
        for source in &recipe.source {
            let donor_dir = resources.join(&source.donor);
            if !donor_dir.is_dir() {
                if missing.insert(source.donor.clone()) {
                    println!("skipped: donor {} not checked out", source.donor);
                }
                skipped += 1;
                continue;
            }
            let path = donor_dir.join(&source.path);
            if !path.is_file() {
                bail!(
                    "recipe {}: donor file {} is missing",
                    recipe.family,
                    path.display()
                );
            }
            gate::check_file(&path, &source.donor, &resources)?;
            checked += 1;
        }
    }
    if checked == 0 {
        println!("skipped: no donor checkouts under {}", resources.display());
    }
    println!(
        "libc-oracle check: {} recipes, {} sources checked, {} skipped",
        loaded.len(),
        checked,
        skipped
    );
    Ok(0)
}

fn command_vendor(args: &[String]) -> Result<u8> {
    let mut donor = None;
    let mut from = None;
    let mut to = None;
    let mut data = false;
    let mut dry_run = false;
    let mut date = None;
    let mut resources = None;
    let mut no_pin_check = false;
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--donor" => donor = Some(value(args, &mut index, "--donor")?),
            "--from" => from = Some(value(args, &mut index, "--from")?),
            "--to" => to = Some(value(args, &mut index, "--to")?),
            "--date" => date = Some(value(args, &mut index, "--date")?),
            "--data" => data = true,
            "--dry-run" => dry_run = true,
            "--no-pin-check" => no_pin_check = true,
            "--resources" => {
                resources = Some(PathBuf::from(value(args, &mut index, "--resources")?))
            }
            flag => bail!("unknown vendor flag `{flag}`"),
        }
        index += 1;
    }
    let Some(donor) = donor else {
        bail!("vendor needs --donor\n{USAGE}");
    };
    let Some(from) = from else {
        bail!("vendor needs --from\n{USAGE}");
    };
    let Some(to) = to else {
        bail!("vendor needs --to\n{USAGE}");
    };
    let resources = resolve_resources(resources)?;
    let options = vendor::VendorOptions {
        donor: &donor,
        from: &from,
        to: Path::new(&to),
        data,
        dry_run,
        date: date.as_deref(),
        resources: &resources,
        no_pin_check,
    };
    vendor::run(&options)
}

#[allow(clippy::too_many_arguments)]
fn build_recipe(
    recipe: &Recipe,
    defaults: &Defaults,
    recipes: &Path,
    resources: &Path,
    out: &Path,
    opt: Vec<String>,
    sanitize: bool,
    no_pin_check: bool,
    adhoc: bool,
) -> Result<()> {
    let request = build::BuildRequest {
        recipe,
        defaults,
        recipes,
        resources,
        out,
        opt,
        sanitize,
        no_pin_check,
        adhoc,
    };
    build::build(&request)?;
    Ok(())
}

fn known_donors(resources: &Path) -> Option<BTreeSet<String>> {
    pin::read_lock(resources)
        .ok()
        .map(|lock| lock.keys().cloned().collect())
}

fn resolve_resources(explicit: Option<PathBuf>) -> Result<PathBuf> {
    if let Some(path) = explicit {
        return Ok(path);
    }
    let current = std::env::current_dir().context("cannot read the current directory")?;
    let base = current.canonicalize().unwrap_or(current);
    for directory in base.ancestors() {
        let candidate = directory.join("ravenbsd-resources");
        if candidate.is_dir() {
            return Ok(candidate);
        }
    }
    bail!(
        "cannot find ravenbsd-resources at or above {}; pass --resources",
        base.display()
    )
}

fn valid_adhoc_path(path: &str, kind: &str) -> bool {
    let extension = if kind == "header" { ".h" } else { ".c" };
    !path.is_empty()
        && path.ends_with(extension)
        && !path.starts_with('/')
        && !path.contains('\\')
        && Path::new(path)
            .components()
            .all(|component| matches!(component, std::path::Component::Normal(_)))
}

fn value(args: &[String], index: &mut usize, flag: &str) -> Result<String> {
    *index += 1;
    args.get(*index)
        .cloned()
        .ok_or_else(|| anyhow::anyhow!("{flag} needs a value"))
}

#[cfg(test)]
#[path = "lib_ut.rs"]
mod tests;
