// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Kernel freshness checks for the QEMU target runner (S750).
//!
//! The runner consumes `build/x86_64/kernel.elf` without rebuilding it. A cached kernel from an
//! older commit pairs an old syscall ABI with the current userland and vfsd, which turns a valid
//! request into a spurious failure. Before a target boot, the runner therefore requires:
//!
//! * the release profile sidecar next to the kernel;
//! * a clean worktree at the commit whose id the kernel embeds; and
//! * the clean short `HEAD` id inside the kernel image.
//!
//! A stale, dirty or missing artifact is refused instead of being booted.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, bail};

/// The commit identity the build embeds.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HeadStamp {
    /// Short `HEAD` hash (`git rev-parse --short HEAD`).
    pub short: String,
    /// Whether the worktree has tracked or untracked changes.
    pub dirty: bool,
}

/// The profile sidecar next to an artifact (`kernel.elf` -> `kernel.elf.profile`).
#[must_use]
pub fn profile_path(kernel: &Path) -> PathBuf {
    let mut name = kernel.as_os_str().to_os_string();
    name.push(".profile");
    PathBuf::from(name)
}

/// Read the commit identity of the worktree at `root`.
pub fn head_stamp(root: &Path) -> Result<HeadStamp> {
    let short = git_output(root, &["rev-parse", "--short", "HEAD"])?;
    if short.is_empty() {
        bail!("git returned an empty short HEAD id for {}", root.display());
    }
    let status = git_output(root, &["status", "--porcelain"])?;
    Ok(HeadStamp {
        short,
        dirty: !status.is_empty(),
    })
}

/// Parse a profile sidecar. Only `release` and `debug` are valid.
#[must_use]
pub fn parse_profile(bytes: &[u8]) -> Option<bool> {
    match std::str::from_utf8(bytes).ok()?.trim() {
        "release" => Some(true),
        "debug" => Some(false),
        _ => None,
    }
}

/// True when `data` contains `short` as a standalone build-id token.
///
/// The embedded id is `<short>[-dirty][ <date>]`. A longer hash that merely contains `short`
/// (preceded or followed by another hex digit) and a `short-dirty` id are not a clean match.
#[must_use]
pub fn contains_clean_build_id(data: &[u8], short: &str) -> bool {
    let needle = short.as_bytes();
    if needle.is_empty() {
        return false;
    }
    data.windows(needle.len())
        .enumerate()
        .any(|(index, window)| {
            if window != needle {
                return false;
            }
            let before_ok = index == 0 || !data[index - 1].is_ascii_hexdigit();
            let after_ok = data
                .get(index + needle.len())
                .is_none_or(|byte| *byte != b'-' && !byte.is_ascii_hexdigit());
            before_ok && after_ok
        })
}

/// True when `data` contains the dirty form of `short` (`<short>-dirty`).
#[must_use]
pub fn contains_dirty_build_id(data: &[u8], short: &str) -> bool {
    let needle = format!("{short}-dirty");
    data.windows(needle.len())
        .any(|window| window == needle.as_bytes())
}

/// Reject a kernel whose profile, worktree or embedded build id is not fresh.
pub fn verify_stamp(
    kernel: &Path,
    data: &[u8],
    profile: Option<&[u8]>,
    head: &HeadStamp,
) -> Result<()> {
    match profile.and_then(parse_profile) {
        Some(true) => {}
        Some(false) => bail!(
            "{} was built with the debug profile; run `make kernel RELEASE=1` for the G1 target",
            kernel.display()
        ),
        None => bail!(
            "{} has no valid release profile stamp ({}); run `make kernel RELEASE=1`",
            kernel.display(),
            profile_path(kernel).display()
        ),
    }
    if head.short.is_empty() || !head.short.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        bail!("invalid HEAD short id `{}`", head.short);
    }
    if head.dirty {
        bail!(
            "release kernel freshness requires a clean worktree, but HEAD is {}-dirty; commit or stash the changes",
            head.short
        );
    }
    if contains_dirty_build_id(data, &head.short) {
        bail!(
            "{} was built from a dirty tree ({}-dirty); rebuild it with `make kernel RELEASE=1`",
            kernel.display(),
            head.short
        );
    }
    if !contains_clean_build_id(data, &head.short) {
        bail!(
            "stale kernel: {} does not embed the clean HEAD id {}; rebuild it with `make kernel RELEASE=1`",
            kernel.display(),
            head.short
        );
    }
    Ok(())
}

/// Verify the kernel artifact the QEMU runner is about to boot.
pub fn verify_kernel(root: &Path, kernel: &Path) -> Result<()> {
    let data = fs::read(kernel)
        .with_context(|| format!("cannot read target kernel {}", kernel.display()))?;
    let profile = fs::read(profile_path(kernel)).ok();
    let head = head_stamp(root)
        .with_context(|| format!("cannot determine kernel freshness for {}", kernel.display()))?;
    verify_stamp(kernel, &data, profile.as_deref(), &head)
}

fn git_output(root: &Path, args: &[&str]) -> Result<String> {
    let output = Command::new("git")
        .args(args)
        .current_dir(root)
        .output()
        .with_context(|| format!("cannot run `git {}`", args.join(" ")))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("git {} failed: {}", args.join(" "), stderr.trim());
    }
    let text = String::from_utf8(output.stdout).context("git output is not UTF-8")?;
    Ok(text.trim().to_string())
}

#[cfg(test)]
#[path = "freshness_ut.rs"]
mod tests;
