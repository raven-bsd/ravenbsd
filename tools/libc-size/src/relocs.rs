// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `libc.so.1` relocation and size measurement (`R-12`, `R-20`, `BP-7`).

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

use crate::Arch;
use crate::measure::{dynamic_info, file_size, parse_berkeley, parse_sections, section_size};
use crate::report::LibcSoRow;
use crate::tools::Tools;

/// The exact status of every dynamic row while `C62` has not landed.
pub const SKIPPED_ABSENT: &str = "skipped: libc.so.1 absent (C62 not landed)";

/// The reason without the `skipped: ` prefix, for [`Row::skipped`](crate::report::Row::skipped).
#[must_use]
pub fn absence_reason() -> &'static str {
    "libc.so.1 absent (C62 not landed)"
}

/// The exact path of the shared libc of one architecture, repository-relative.
#[must_use]
pub fn so_path(arch: Arch) -> PathBuf {
    PathBuf::from(format!("build/{}/libc.so.1", arch.short()))
}

/// The skipped row of one architecture, with the exact path.
#[must_use]
pub fn skipped_row(arch: Arch) -> LibcSoRow {
    LibcSoRow {
        arch,
        path: so_path(arch).display().to_string(),
        s: 0,
        bss: 0,
        file: 0,
        needed: 0,
        jump_slot: 0,
        glob_dat: 0,
        relative: 0,
        data_rel_ro: 0,
        status: SKIPPED_ABSENT.to_string(),
    }
}

/// Whether the shared libc exists.
#[must_use]
pub fn present(libc_dir: &Path) -> bool {
    libc_dir.join("libc.so.1").is_file()
}

/// Measure one `libc.so.1`.
///
/// # Errors
/// A missing file or a tool failure.
pub fn measure(
    tools: &Tools,
    root: &Path,
    arch: Arch,
    libc_dir: &Path,
    relative: &str,
) -> Result<LibcSoRow> {
    let path = libc_dir.join("libc.so.1");
    if !path.is_file() {
        return Ok(skipped_row(arch));
    }
    let size_output = run(tools, &tools.size, &[path.display().to_string()], root)?;
    let entry = parse_berkeley(&size_output)?
        .into_iter()
        .next()
        .context("llvm-size returned no row")?;
    let sections_output = run(
        tools,
        &tools.size,
        &["-A".to_string(), path.display().to_string()],
        root,
    )?;
    let sections = parse_sections(&sections_output)?;
    let dynamic = run(
        tools,
        &tools.readobj,
        &[
            "--elf-output-style=GNU".to_string(),
            "-d".to_string(),
            path.display().to_string(),
        ],
        root,
    )?;
    let relocations = run(
        tools,
        &tools.readobj,
        &[
            "--elf-output-style=GNU".to_string(),
            "-r".to_string(),
            path.display().to_string(),
        ],
        root,
    )?;
    let info = dynamic_info(&dynamic, &relocations);
    Ok(LibcSoRow {
        arch,
        path: relative.to_string(),
        s: entry.size.s(),
        bss: entry.size.bss,
        file: file_size(&path)?,
        needed: u64::try_from(info.needed).unwrap_or(u64::MAX),
        jump_slot: u64::try_from(info.jump_slot).unwrap_or(u64::MAX),
        glob_dat: u64::try_from(info.glob_dat).unwrap_or(u64::MAX),
        relative: u64::try_from(info.relative).unwrap_or(u64::MAX),
        data_rel_ro: section_size(&sections, ".data.rel.ro"),
        status: "ok".to_string(),
    })
}

fn run(_tools: &Tools, program: &Path, args: &[String], root: &Path) -> Result<String> {
    let spec = crate::build::CommandSpec::new(program, args.to_vec(), root.to_path_buf());
    let output = spec.run()?;
    if !output.status.success() {
        anyhow::bail!(
            "{} failed: {}",
            program.display(),
            crate::build::compact_error(&String::from_utf8_lossy(&output.stderr))
        );
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

/// Render one row.
#[must_use]
pub fn render(row: &LibcSoRow) -> String {
    format!(
        "libc-size relocs: {} {} S={} B={} F={} needed={} jump_slot={} glob_dat={} relative={} data.rel.ro={}",
        row.arch.short(),
        row.path,
        row.s,
        row.bss,
        row.file,
        row.needed,
        row.jump_slot,
        row.glob_dat,
        row.relative,
        row.data_rel_ro
    )
}

#[cfg(test)]
#[path = "relocs_ut.rs"]
mod tests;
