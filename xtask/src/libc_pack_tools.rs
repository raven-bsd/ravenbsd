// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//!
//! Tool discovery (C60 4.2): `llvm-ar llvm-nm llvm-size llvm-readelf llvm-objdump` from
//! `$LLVM_BIN_DIR`, the rustc sysroot's `llvm-tools`, or `PATH`; `ld.lld` from `$LD_LLD`,
//! `PATH`, or `<sysroot>/.../gcc-ld/ld.lld`. GNU `ar`/`nm`/`readelf` are the documented
//! fallback.

use std::fmt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use anyhow::{Context, Result, bail};

/// A tool that is genuinely absent.
#[derive(Debug)]
pub struct MissingTool(pub String);

impl fmt::Display for MissingTool {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{} not found", self.0)
    }
}

impl std::error::Error for MissingTool {}

/// The tool set of one arch.
pub struct Tools {
    /// Archive tool.
    pub ar: PathBuf,
    /// Symbol lister.
    pub nm: PathBuf,
    /// Size reporter.
    pub size: PathBuf,
    /// Linker.
    pub lld: PathBuf,
}

impl Tools {
    /// Locate every tool, or fail with [`MissingTool`].
    ///
    /// # Errors
    /// A missing tool.
    pub fn discover(root: &Path, _arch: crate::libc_pack::Arch) -> Result<Self> {
        let sysroot = sysroot()?;
        let host = host_triple();
        let llvm_dir = std::env::var_os("LLVM_BIN_DIR").map(PathBuf::from);
        let sysroot_bin = sysroot.join("lib/rustlib").join(&host).join("bin");
        let find = |names: &[&str], env: &str| -> Result<PathBuf> {
            if let Ok(value) = std::env::var(env)
                && !value.is_empty()
            {
                return Ok(PathBuf::from(value));
            }
            for name in names {
                if let Some(dir) = &llvm_dir {
                    let candidate = dir.join(name);
                    if candidate.is_file() {
                        return Ok(candidate);
                    }
                }
                let candidate = sysroot_bin.join(name);
                if candidate.is_file() {
                    return Ok(candidate);
                }
                if let Ok(found) = which(name) {
                    return Ok(found);
                }
            }
            bail!(
                "{}",
                MissingTool(names.first().copied().unwrap_or("tool").to_string())
            );
        };
        let ar = find(&["llvm-ar", "ar"], "LLVM_AR")?;
        let nm = find(&["llvm-nm", "nm"], "LLVM_NM")?;
        let size = find(&["llvm-size", "size"], "LLVM_SIZE")?;
        let readelf = find(&["llvm-readelf", "readelf"], "LLVM_READELF")?;
        let _ = &readelf;
        let lld = if let Ok(value) = std::env::var("LD_LLD")
            && !value.is_empty()
        {
            PathBuf::from(value)
        } else if let Ok(found) = which("ld.lld") {
            found
        } else {
            let candidate = sysroot_bin.join("gcc-ld/ld.lld");
            if candidate.is_file() {
                candidate
            } else {
                bail!("{}", MissingTool("ld.lld".to_string()));
            }
        };
        let _ = root;
        Ok(Self { ar, nm, size, lld })
    }

    /// Extract every member, sorted.
    ///
    /// # Errors
    /// A tool or I/O failure.
    pub fn extract(&self, archive: &Path, dir: &Path) -> Result<Vec<String>> {
        let listing = capture(&self.ar, &["t"], &archive.to_string_lossy(), dir)?;
        let mut members = listing
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .map(str::to_string)
            .collect::<Vec<_>>();
        members.sort();
        let status = Command::new(&self.ar)
            .args(["x"])
            .arg(archive)
            .current_dir(dir)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .with_context(|| format!("cannot run {}", self.ar.display()))?;
        if !status.success() {
            bail!("{} x failed", self.ar.display());
        }
        Ok(members)
    }

    /// Pack the members deterministically.
    ///
    /// # Errors
    /// A tool failure.
    pub fn pack(&self, out: &Path, members: &[String], dir: &Path) -> Result<()> {
        if let Some(parent) = out.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let out = std::path::absolute(out)?;
        let status = Command::new(&self.ar)
            .args(["rcsD"])
            .arg(&out)
            .args(members)
            .current_dir(dir)
            .stdout(Stdio::null())
            .status()
            .with_context(|| format!("cannot run {}", self.ar.display()))?;
        if !status.success() {
            bail!("{} rcsD failed", self.ar.display());
        }
        Ok(())
    }

    /// Write the member/symbol map.
    ///
    /// # Errors
    /// A tool or I/O failure.
    pub fn map_json(
        &self,
        archive: &Path,
        members: &[String],
        dir: &Path,
        out: &Path,
    ) -> Result<()> {
        let listing = capture(
            &self.nm,
            &["--defined-only", "--extern-only", "--format=posix"],
            &std::path::absolute(archive)?.to_string_lossy(),
            dir,
        )?;
        let symbols = parse_nm(&listing);
        let mut json = String::from("{\"members\":[");
        for (index, member) in members.iter().enumerate() {
            if index > 0 {
                json.push(',');
            }
            let defined = symbols
                .iter()
                .filter(|(owner, _)| owner == member)
                .map(|(_, name)| format!("\"{name}\""))
                .collect::<Vec<_>>()
                .join(",");
            json.push_str(&format!(
                "{{\"name\":\"{member}\",\"defined\":[{defined}],\"undefined\":[]}}"
            ));
        }
        json.push_str("]}\n");
        std::fs::write(out, json).with_context(|| format!("cannot write {}", out.display()))?;
        Ok(())
    }

    /// The size program.
    #[must_use]
    pub fn size(&self) -> &Path {
        &self.size
    }

    /// The linker.
    #[must_use]
    pub fn lld(&self) -> &Path {
        &self.lld
    }
}

fn sysroot() -> Result<PathBuf> {
    let output = Command::new("rustc")
        .args(["--print", "sysroot"])
        .output()
        .context("cannot run rustc --print sysroot")?;
    if !output.status.success() {
        bail!("rustc --print sysroot failed");
    }
    Ok(PathBuf::from(
        String::from_utf8_lossy(&output.stdout).trim().to_string(),
    ))
}

fn host_triple() -> String {
    let output = Command::new("rustc")
        .args(["-vV"])
        .output()
        .ok()
        .map(|output| String::from_utf8_lossy(&output.stdout).into_owned())
        .unwrap_or_default();
    for line in output.lines() {
        if let Some(value) = line.strip_prefix("host: ") {
            return value.trim().to_string();
        }
    }
    "x86_64-unknown-linux-gnu".to_string()
}

fn which(name: &str) -> Result<PathBuf> {
    let output = Command::new("sh")
        .args(["-c", &format!("command -v {name}")])
        .output()
        .context("cannot run command -v")?;
    let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if path.is_empty() {
        bail!("{}", MissingTool(name.to_string()));
    }
    Ok(PathBuf::from(path))
}

fn capture(program: &Path, args: &[&str], last: &str, dir: &Path) -> Result<String> {
    let output = Command::new(program)
        .args(args)
        .arg(last)
        .current_dir(dir)
        .output()
        .with_context(|| format!("cannot run {}", program.display()))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("{} failed: {}", program.display(), stderr.trim());
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

fn parse_nm(text: &str) -> Vec<(String, String)> {
    let mut member = String::new();
    let mut out = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        if line.ends_with(':') {
            let header = line.trim_end_matches(':');
            member = header
                .rsplit_once('[')
                .and_then(|(_, rest)| rest.strip_suffix(']'))
                .unwrap_or(header)
                .to_string();
            continue;
        }
        let mut fields = line.split_whitespace();
        let (Some(name), Some(kind), Some(value)) = (fields.next(), fields.next(), fields.next())
        else {
            continue;
        };
        if kind.len() != 1 || u64::from_str_radix(value, 16).is_err() {
            continue;
        }
        out.push((member.clone(), name.to_string()));
    }
    out
}

/// Run the host `mkmap` and return its first `mkmap:` line.
///
/// # Errors
/// A missing binary or a failed run.
pub(crate) fn mkmap(root: &Path, args: &[String]) -> Result<String> {
    let output = Command::new("cargo")
        .args(["run", "-q", "-p", "raven-libc-mkmap", "--"])
        .args(args)
        .current_dir(root)
        .output()
        .context("cannot run raven-libc-mkmap")?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    if !output.status.success() {
        bail!("mkmap failed: {}", stderr.trim());
    }
    let summary = stdout
        .lines()
        .find(|line| line.starts_with("mkmap:"))
        .unwrap_or("mkmap: ok")
        .to_string();
    Ok(summary)
}
