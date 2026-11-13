// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//!
//! Host-run end-to-end test (C60 4.8): `x86_64-unknown-none` objects executed natively on a
//! Linux x86_64 host. Programs present under `libc/ravenlibc/crates/raven-libc/hostrun/` are
//! built, linked against `libc.a` and run; the exit code is the index of the first failed check.

use std::path::Path;
use std::process::Command;

use anyhow::{Context, Result};

use crate::libc_pack::Arch;
use crate::libc_pack_tools::Tools;

/// Run every host program that exists for this arch.
pub(crate) fn run(root: &Path, arch: &Arch, out: &Path) -> Result<()> {
    if *arch != Arch::X86_64 || !cfg!(target_os = "linux") {
        println!("libc-pack: host-run: skipped(host)");
        return Ok(());
    }
    let tools = Tools::discover(root, *arch)?;
    let archive = out.join("libc.a");
    if !archive.is_file() {
        println!("libc-pack: host-run: skipped(no libc.a)");
        return Ok(());
    }
    let dir = out.join("hostrun");
    std::fs::create_dir_all(&dir)?;
    let programs = [
        "h1_string",
        "h2_stdlib",
        "h3_format",
        "h4_ctype",
        "h5_time",
        "h6_math",
    ];
    let mut ran = 0usize;
    for name in programs {
        let source = root
            .join("libc/ravenlibc/crates/raven-libc/hostrun")
            .join(format!("{name}.rs"));
        if !source.is_file() {
            println!("libc-pack: host-run {name}: skipped(no source)");
            continue;
        }
        let object = dir.join(format!("{name}.o"));
        let binary = dir.join(name);
        let compile = Command::new("rustc")
            .args(["--edition", "2024", "--crate-type", "lib", "--emit=obj"])
            .args(["--target", arch.triple()])
            .args([
                "-C",
                "panic=abort",
                "-C",
                "opt-level=z",
                "-C",
                "relocation-model=pic",
            ])
            .arg("-o")
            .arg(&object)
            .arg(&source)
            .output()
            .context("cannot run rustc for a host-run program")?;
        if !compile.status.success() {
            println!(
                "libc-pack: host-run {name}: FAILED ({})",
                String::from_utf8_lossy(&compile.stderr).trim()
            );
            continue;
        }
        let link = Command::new(tools.lld())
            .args(["-static", "--gc-sections", "-e", "_start"])
            .arg(&object)
            .arg(&archive)
            .arg("-o")
            .arg(&binary)
            .output()
            .context("cannot run ld.lld for a host-run program")?;
        if !link.status.success() {
            println!(
                "libc-pack: host-run {name}: FAILED ({})",
                String::from_utf8_lossy(&link.stderr).trim()
            );
            continue;
        }
        let status = Command::new(&binary)
            .status()
            .with_context(|| format!("cannot run {name}"))?;
        if status.success() {
            println!("libc-pack: host-run {name}: ok");
            ran = ran.saturating_add(1);
        } else {
            println!(
                "libc-pack: host-run {name}: FAILED (exit {})",
                status.code().unwrap_or(-1)
            );
        }
    }
    println!("libc-pack: host-run: {ran} program(s) passed");
    Ok(())
}
