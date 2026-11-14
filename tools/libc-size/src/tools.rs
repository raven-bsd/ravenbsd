// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Tool discovery (`C69` 4.1, section 2 item 5).
//!
//! Lookup order: the environment override, then
//! `$(rustc --print sysroot)/lib/rustlib/<host>/bin`, then `PATH`. A missing tool is
//! [`MissingTool`]; the CLI turns that into exit 77 with `skipped: <tool> not found`, as
//! `C35` does. `RL_SIZE_REQUIRED=1` turns a skip into a failure at the CLI layer.

use std::fmt;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result};

/// A tool that is genuinely absent.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MissingTool {
    /// The name that was searched.
    pub name: String,
}

impl fmt::Display for MissingTool {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{} not found", self.name)
    }
}

impl std::error::Error for MissingTool {}

/// The host tool set used by the gate.
#[derive(Clone, Debug)]
pub struct Tools {
    /// `llvm-size`.
    pub size: PathBuf,
    /// `llvm-nm`.
    pub nm: PathBuf,
    /// `llvm-readobj`.
    pub readobj: PathBuf,
    /// `llvm-objdump`.
    pub objdump: PathBuf,
    /// `llvm-strip`.
    pub strip: PathBuf,
    /// `llvm-ar`.
    pub ar: PathBuf,
    /// `llvm-objcopy`.
    pub objcopy: PathBuf,
    /// `ld.lld`.
    pub lld: PathBuf,
    /// `rustc`.
    pub rustc: PathBuf,
    /// `cargo`.
    pub cargo: PathBuf,
    /// `clang`, when the C rows can be built.
    pub clang: Option<PathBuf>,
    /// First line of `rustc --version`.
    pub rustc_version: String,
    /// First line of `ld.lld --version`.
    pub lld_version: String,
}

impl Tools {
    /// Locate every required tool.
    ///
    /// # Errors
    /// [`MissingTool`] when one is absent; a process error when `rustc` cannot run.
    pub fn discover() -> Result<Self> {
        let rustc = PathBuf::from(env_or("RUSTC", "rustc"));
        let sysroot = command_output(&rustc, &["--print", "sysroot"])?;
        let host = host_triple(&rustc)?;
        let llvm_bin = PathBuf::from(sysroot.trim())
            .join("lib/rustlib")
            .join(&host)
            .join("bin");
        let gcc_ld = llvm_bin.join("gcc-ld");

        let size = find_tool("LLVM_SIZE", "llvm-size", std::slice::from_ref(&llvm_bin))?;
        let nm = find_tool("LLVM_NM", "llvm-nm", std::slice::from_ref(&llvm_bin))?;
        let readobj = find_tool(
            "LLVM_READOBJ",
            "llvm-readobj",
            std::slice::from_ref(&llvm_bin),
        )?;
        let objdump = find_tool(
            "LLVM_OBJDUMP",
            "llvm-objdump",
            std::slice::from_ref(&llvm_bin),
        )?;
        let strip = find_tool("LLVM_STRIP", "llvm-strip", std::slice::from_ref(&llvm_bin))?;
        let ar = find_tool("LLVM_AR", "llvm-ar", std::slice::from_ref(&llvm_bin))?;
        let objcopy = find_tool(
            "LLVM_OBJCOPY",
            "llvm-objcopy",
            std::slice::from_ref(&llvm_bin),
        )?;
        let lld = find_tool("LD_LLD", "ld.lld", &[gcc_ld])?;
        let clang = find_optional("CLANG", "clang", &[]);
        let cargo = PathBuf::from(env_or("CARGO", "cargo"));
        let rustc_version = first_line(&command_output(&rustc, &["--version"])?);
        let lld_version = first_line(&command_output(&lld, &["--version"])?);
        Ok(Self {
            size,
            nm,
            readobj,
            objdump,
            strip,
            ar,
            objcopy,
            lld,
            rustc,
            cargo,
            clang,
            rustc_version,
            lld_version,
        })
    }

    /// The toolchain block of a result file.
    #[must_use]
    pub fn toolchain(&self) -> crate::report::Toolchain {
        crate::report::Toolchain {
            rustc: self.rustc_version.clone(),
            lld: self.lld_version.clone(),
        }
    }

    /// True when the C probe rows can be built.
    #[must_use]
    pub fn has_c_toolchain(&self) -> bool {
        self.clang.is_some()
    }
}

/// Whether a skip is a hard failure (`RL_SIZE_REQUIRED=1`).
#[must_use]
pub fn required() -> bool {
    std::env::var("RL_SIZE_REQUIRED").is_ok_and(|value| value == "1")
}

fn env_or(name: &str, fallback: &str) -> String {
    std::env::var(name)
        .ok()
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| fallback.to_string())
}

/// Find a tool, or return [`MissingTool`].
///
/// # Errors
/// The tool is absent.
pub fn find_tool(env: &str, name: &str, dirs: &[PathBuf]) -> Result<PathBuf> {
    if let Some(found) = locate(env, name, dirs) {
        return Ok(found);
    }
    Err(anyhow::Error::new(MissingTool {
        name: name.to_string(),
    }))
}

fn find_optional(env: &str, name: &str, dirs: &[PathBuf]) -> Option<PathBuf> {
    locate(env, name, dirs)
}

/// The pure lookup rule, testable without touching the environment.
#[must_use]
pub fn locate_with(
    override_value: Option<&str>,
    name: &str,
    dirs: &[PathBuf],
    path_dirs: &[PathBuf],
) -> Option<PathBuf> {
    if let Some(value) = override_value
        && !value.is_empty()
    {
        return Some(PathBuf::from(value));
    }
    for dir in dirs {
        let candidate = dir.join(name);
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    for dir in path_dirs {
        let candidate = dir.join(name);
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}

fn locate(env: &str, name: &str, dirs: &[PathBuf]) -> Option<PathBuf> {
    let override_value = std::env::var(env).ok();
    let path_dirs: Vec<PathBuf> = std::env::var_os("PATH")
        .map(|value| std::env::split_paths(&value).collect())
        .unwrap_or_default();
    locate_with(override_value.as_deref(), name, dirs, &path_dirs)
}

fn host_triple(rustc: &Path) -> Result<String> {
    let output = command_output(rustc, &["-vV"])?;
    for line in output.lines() {
        if let Some(host) = line.strip_prefix("host: ") {
            return Ok(host.trim().to_string());
        }
    }
    anyhow::bail!("rustc -vV has no host line")
}

fn command_output(program: &Path, args: &[&str]) -> Result<String> {
    let output = Command::new(program)
        .args(args)
        .output()
        .with_context(|| format!("cannot run {}", program.display()))?;
    if !output.status.success() {
        anyhow::bail!(
            "{} {} failed: {}",
            program.display(),
            args.join(" "),
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

fn first_line(text: &str) -> String {
    text.lines().next().unwrap_or_default().trim().to_string()
}

#[cfg(test)]
#[path = "tools_ut.rs"]
mod tests;
