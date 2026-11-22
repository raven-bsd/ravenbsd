// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Committed POSIX interface declaration sweep.
//!
//! The sweep compiles every `present` profile interface row with the same feature macros as the
//! gate (`-std=c17 -D_POSIX_C_SOURCE=202405L -D_XOPEN_SOURCE=800`) and reports exact failures.
//! A failure for a name in the extension list is reported as `EXTENSION`; a failure for any other
//! name is a required-declaration regression and exits nonzero. `--strict` turns the documented
//! extensions into failures too, so P1f7 can flip the check once the waiver list is gone.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, bail};

use crate::gate::probe_source;
use crate::model::{Status, load_profile};

/// The committed extension waiver list.
pub const DEFAULT_EXTENSIONS: &str = "tests/libc/p1c-unistd/interface-extensions.txt";

/// One interface row's sweep verdict.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Class {
    /// The declaration compiled.
    Pass,
    /// The declaration failed and the name is in the documented extension list.
    Extension,
    /// The declaration failed and the name is required or strict mode is on.
    Required,
}

/// Read the extension list: one name per line, `#` starts a comment, blank lines are ignored.
pub(crate) fn load_extensions(path: &Path) -> Result<BTreeSet<String>> {
    let text = std::fs::read_to_string(path)
        .with_context(|| format!("cannot read extension list {}", path.display()))?;
    let mut names = BTreeSet::new();
    for line in text.lines() {
        let name = line.split('#').next().unwrap_or("").trim();
        if !name.is_empty() {
            names.insert(name.to_string());
        }
    }
    Ok(names)
}

/// Classify a row from its compile result and the extension list.
pub(crate) fn classify(
    name: &str,
    failed: bool,
    extensions: &BTreeSet<String>,
    strict: bool,
) -> Class {
    if !failed {
        Class::Pass
    } else if extensions.contains(name) && !strict {
        Class::Extension
    } else {
        Class::Required
    }
}

/// Pick the compiler and architecture include directory.
pub(crate) fn compiler(arch: &str) -> Result<(String, PathBuf)> {
    match arch {
        "x86_64" => Ok((
            std::env::var("CC")
                .ok()
                .filter(|value| !value.trim().is_empty())
                .unwrap_or_else(|| "cc".to_string()),
            PathBuf::from("libc/ravenlibc/arch/x86_64/include"),
        )),
        "aarch64" => Ok((
            std::env::var("AARCH64_CC")
                .ok()
                .filter(|value| !value.trim().is_empty())
                .unwrap_or_else(|| "aarch64-linux-gnu-gcc".to_string()),
            PathBuf::from("libc/ravenlibc/arch/aarch64/include"),
        )),
        other => bail!("unknown architecture `{other}` (expected x86_64 or aarch64)"),
    }
}

/// Run the full present-interface sweep for `arch`.
///
/// # Errors
///
/// Returns an error when the profile or extension list cannot be read, the compiler cannot be
/// executed, or a probe file cannot be written.
pub fn run(root: &Path, arch: &str, extensions_path: &Path, strict: bool) -> Result<u8> {
    let profile = load_profile(&crate::mgmt::posix_dir(root)?.join("profile.toml"))
        .context("cannot load profile")?;
    let extensions = load_extensions(extensions_path)?;
    let (cc, arch_include) = compiler(arch)?;
    let include_root = root.join("libc/ravenlibc/include");
    let include_arch = root.join(&arch_include);
    let output_dir = root.join("build/posix-sweep").join(arch);
    std::fs::create_dir_all(&output_dir)
        .with_context(|| format!("cannot create {}", output_dir.display()))?;

    let mut pass = 0usize;
    let mut extension = 0usize;
    let mut required = 0usize;
    for (index, row) in profile
        .interface
        .iter()
        .filter(|row| row.meta.status == Status::Present)
        .enumerate()
    {
        let source = probe_source(row);
        let path = output_dir.join(format!("interface_{index}.c"));
        std::fs::write(&path, source)
            .with_context(|| format!("cannot write {}", path.display()))?;
        let output = Command::new(&cc)
            .arg("-I")
            .arg(&include_root)
            .arg("-I")
            .arg(&include_arch)
            .arg("-fsyntax-only")
            .arg("-std=c17")
            .arg("-D_POSIX_C_SOURCE=202405L")
            .arg("-D_XOPEN_SOURCE=800")
            .arg("-Wno-unused-value")
            .arg(&path)
            .output()
            .with_context(|| format!("cannot run `{cc}` for the {arch} sweep"))?;
        let failed = !output.status.success();
        let detail = String::from_utf8_lossy(&output.stderr);
        let detail = detail.lines().next().unwrap_or("").trim();
        match classify(&row.name, failed, &extensions, strict) {
            Class::Pass => pass += 1,
            Class::Extension => {
                extension += 1;
                println!("EXTENSION {} <{}>", row.meta.id, row.header);
            }
            Class::Required => {
                required += 1;
                println!("FAIL {} <{}>: {detail}", row.meta.id, row.header);
            }
        }
    }
    println!(
        "interface-sweep: {arch}: {pass} pass, {extension} extension, {required} required fail"
    );
    Ok(u8::from(required > 0))
}
