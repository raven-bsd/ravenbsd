// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `raven-libc-hdrgen`: generate, check and verify the ravenlibc ABI tables.
//!
//! Exit codes: 0 success, 1 a check failed, 2 usage or I/O error, 77 a required tool is
//! missing and `RL_ABI_REQUIRED` is unset.

#![forbid(unsafe_code)]

mod layout;
mod model;
mod probe;
mod provenance;
mod render_c;
mod render_rust;
mod syscall;
mod validate;
mod values;

use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};

use crate::validate::Tables;

const USAGE: &str = "\
usage:
  raven-libc-hdrgen generate   [--abi DIR] [--rust DIR] [--root DIR] [--resources DIR]
  raven-libc-hdrgen check      [--abi DIR] [--rust DIR] [--root DIR] [--resources DIR]
  raven-libc-hdrgen verify     [--target x86_64|aarch64|riscv64|all] [--resources DIR]
                               [--clang CLANG] [--sed SED] [--root DIR]
  raven-libc-hdrgen provenance --date YYYY-MM-DD [--resources DIR] [--root DIR]
";

fn main() -> std::process::ExitCode {
    match run(std::env::args().skip(1).collect()) {
        Ok(code) => std::process::ExitCode::from(code),
        Err(error) => {
            eprintln!("raven-libc-hdrgen: {error:#}");
            let code = if error.downcast_ref::<TableError>().is_some() {
                1
            } else {
                2
            };
            std::process::ExitCode::from(code)
        }
    }
}

/// A table validation failure: a check failed (exit 1), not an I/O error (exit 2).
#[derive(Debug)]
struct TableError {
    message: String,
}

impl std::fmt::Display for TableError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for TableError {}

fn run(args: Vec<String>) -> Result<u8> {
    let Some(command) = args.first() else {
        print!("{USAGE}");
        return Ok(2);
    };
    let rest = &args[1..];
    match command.as_str() {
        "generate" => {
            let options = Options::parse(rest)?;
            generate(&options)
        }
        "check" => {
            let options = Options::parse(rest)?;
            check(&options)
        }
        "verify" => {
            let options = Options::parse(rest)?;
            verify(&options)
        }
        "provenance" => {
            let options = Options::parse(rest)?;
            provenance(&options)
        }
        "-h" | "--help" | "help" => {
            print!("{USAGE}");
            Ok(0)
        }
        other => {
            eprintln!("raven-libc-hdrgen: unknown command {other:?}");
            print!("{USAGE}");
            Ok(2)
        }
    }
}

#[derive(Debug, Default)]
struct Options {
    root: Option<PathBuf>,
    abi: Option<PathBuf>,
    rust: Option<PathBuf>,
    resources: Option<PathBuf>,
    target: Option<String>,
    clang: Option<String>,
    sed: Option<String>,
    date: Option<String>,
}

impl Options {
    fn parse(args: &[String]) -> Result<Self> {
        let mut options = Self::default();
        let mut index = 0;
        while index < args.len() {
            let flag = args[index].clone();
            let value = |index: &mut usize| -> Result<String> {
                *index += 1;
                args.get(*index)
                    .cloned()
                    .with_context(|| format!("{flag} needs a value"))
            };
            match flag.as_str() {
                "--root" => options.root = Some(PathBuf::from(value(&mut index)?)),
                "--abi" => options.abi = Some(PathBuf::from(value(&mut index)?)),
                "--rust" => options.rust = Some(PathBuf::from(value(&mut index)?)),
                "--resources" => options.resources = Some(PathBuf::from(value(&mut index)?)),
                "--target" => options.target = Some(value(&mut index)?),
                "--clang" => options.clang = Some(value(&mut index)?),
                "--sed" => options.sed = Some(value(&mut index)?),
                "--date" => options.date = Some(value(&mut index)?),
                other => bail!("unknown option {other:?}"),
            }
            index += 1;
        }
        Ok(options)
    }

    fn root(&self) -> Result<PathBuf> {
        match &self.root {
            Some(root) => Ok(root.clone()),
            None => std::env::current_dir().context("cannot read the current directory"),
        }
    }

    fn abi(&self) -> Result<PathBuf> {
        Ok(match &self.abi {
            Some(path) => path.clone(),
            None => self.root()?.join("libc/ravenlibc/abi"),
        })
    }

    fn rust(&self) -> Result<PathBuf> {
        Ok(match &self.rust {
            Some(path) => path.clone(),
            None => self
                .root()?
                .join("libc/ravenlibc/crates/raven-libc-abi/src/generated"),
        })
    }

    fn resources(&self) -> Result<PathBuf> {
        if let Some(path) = &self.resources {
            return Ok(path.clone());
        }
        let root = self.root()?;
        provenance::discover_resources(&root).with_context(|| {
            format!(
                "cannot find ravenbsd-resources/donors.lock at or above {}; pass --resources",
                root.display()
            )
        })
    }
}

fn load(options: &Options) -> Result<(Tables, provenance::Lock)> {
    let abi = options.abi()?;
    let lock = provenance::read_lock(&options.resources()?)?;
    let tables = validate::load(&abi).map_err(|error| TableError {
        message: format!("{error:#}"),
    })?;
    Ok((tables, lock))
}

fn generated_rust(
    tables: &Tables,
    values: &validate::Values,
    lock: &provenance::Lock,
    rust_dir: &Path,
) -> Result<Vec<(PathBuf, String)>> {
    let files = render_rust::render_all(tables, values, &lock.musl)?;
    Ok(files
        .into_iter()
        .map(|(name, text)| (rust_dir.join(name), text))
        .collect())
}

fn generated_headers(
    tables: &Tables,
    values: &validate::Values,
    lock: &provenance::Lock,
    abi: &Path,
) -> Result<Vec<(PathBuf, String)>> {
    let mut files = Vec::new();
    for (name, text) in render_c::render_all(tables, values, &lock.musl)? {
        files.push((abi.join("generated/include").join(name), text));
    }
    for (name, text) in syscall::render_all(&lock.linux_raw_sys)? {
        files.push((abi.join("generated/include").join(name), text));
    }
    Ok(files)
}

fn write_files(files: &[(PathBuf, String)]) -> Result<()> {
    for (path, text) in files {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("cannot create {}", parent.display()))?;
        }
        std::fs::write(path, text).with_context(|| format!("cannot write {}", path.display()))?;
    }
    Ok(())
}

fn generate(options: &Options) -> Result<u8> {
    let (tables, lock) = load(options)?;
    let values = validate::validate(&tables)?;
    let mut files = generated_rust(&tables, &values, &lock, &options.rust()?)?;
    files.extend(generated_headers(&tables, &values, &lock, &options.abi()?)?);
    write_files(&files)?;
    eprintln!(
        "raven-libc-hdrgen: wrote {} generated files ({} constants, {} records, {} opaque)",
        files.len(),
        values.by_name.len(),
        tables.structs.record.len(),
        tables.structs.opaque.len()
    );
    Ok(0)
}

fn check(options: &Options) -> Result<u8> {
    let (tables, lock) = load(options)?;
    let values = validate::validate(&tables)?;
    let rust_dir = options.rust()?;
    let abi = options.abi()?;
    let mut expected = generated_rust(&tables, &values, &lock, &rust_dir)?;
    expected.extend(generated_headers(&tables, &values, &lock, &abi)?);
    let mut failures = Vec::new();
    let mut paths = std::collections::BTreeSet::new();
    for (path, text) in &expected {
        paths.insert(path.clone());
        match std::fs::read_to_string(path) {
            Ok(committed) if committed == *text => {}
            Ok(_) => failures.push(format!("stale generated file: {}", path.display())),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                failures.push(format!("missing generated file: {}", path.display()));
            }
            Err(error) => failures.push(format!("cannot read {}: {error}", path.display())),
        }
    }
    for extra in extra_generated(&rust_dir, &abi)? {
        if !paths.contains(&extra) {
            failures.push(format!("extra generated file: {}", extra.display()));
        }
    }
    if failures.is_empty() {
        eprintln!(
            "raven-libc-hdrgen: check ok ({} generated files byte-identical)",
            expected.len()
        );
        Ok(0)
    } else {
        for failure in &failures {
            eprintln!("raven-libc-hdrgen: {failure}");
        }
        eprintln!(
            "raven-libc-hdrgen: check failed ({} problem(s))",
            failures.len()
        );
        Ok(1)
    }
}

fn extra_generated(rust_dir: &Path, abi: &Path) -> Result<Vec<PathBuf>> {
    let mut extra = Vec::new();
    if let Ok(entries) = std::fs::read_dir(rust_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().is_some_and(|extension| extension == "rs") {
                extra.push(path);
            }
        }
    }
    let include = abi.join("generated/include");
    walk_headers(&include, &mut extra)?;
    extra.sort();
    Ok(extra)
}

fn walk_headers(directory: &Path, files: &mut Vec<PathBuf>) -> Result<()> {
    let Ok(entries) = std::fs::read_dir(directory) else {
        return Ok(());
    };
    let mut entries: Vec<PathBuf> = entries.flatten().map(|entry| entry.path()).collect();
    entries.sort();
    for path in entries {
        if path.is_dir() {
            walk_headers(&path, files)?;
        } else if path.extension().is_some_and(|extension| extension == "h") {
            files.push(path);
        }
    }
    Ok(())
}

fn verify(options: &Options) -> Result<u8> {
    let (tables, _lock) = load(options)?;
    let values = validate::validate(&tables)?;
    let clang = options.clang.clone().unwrap_or_else(|| "clang".to_string());
    let sed = options.sed.clone().unwrap_or_else(|| "sed".to_string());
    let Some((clang, sed)) = probe::tools(&clang, &sed)? else {
        return Ok(77);
    };
    let target = options.target.clone().unwrap_or_else(|| "all".to_string());
    let report = probe::verify(
        &tables,
        &values,
        &target,
        &options.abi()?,
        &options.resources()?,
        &clang,
        &sed,
    )?;
    for line in &report.lines {
        println!("{line}");
    }
    if !report.failures.is_empty() {
        for failure in &report.failures {
            eprintln!("FAIL: {failure}");
        }
        eprintln!(
            "raven-libc-hdrgen: verify failed ({} failure(s))",
            report.failures.len()
        );
        return Ok(1);
    }
    if let Some(reason) = &report.skip {
        eprintln!("{reason}");
        return Ok(77);
    }
    eprintln!("raven-libc-hdrgen: verify ok");
    Ok(0)
}

fn provenance(options: &Options) -> Result<u8> {
    let date = options
        .date
        .clone()
        .context("provenance needs --date YYYY-MM-DD")?;
    provenance::validate_date(&date)?;
    let (tables, lock) = load(options)?;
    let entries = provenance::import_entries(&tables, &date, &lock);
    let mut out = String::new();
    for (index, entry) in entries.iter().enumerate() {
        if index > 0 {
            out.push('\n');
        }
        out.push_str(&entry.render());
    }
    print!("{out}");
    Ok(0)
}

#[cfg(test)]
#[path = "main_ut.rs"]
mod tests;
