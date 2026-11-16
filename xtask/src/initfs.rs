// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Initial filesystem packing and profile-driven user builds.

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use raven_initfs::InitfsBuilder;

use crate::build::{self, Arch, Component};
use crate::profiles::Profile;
use crate::repo::Workspace;
use crate::uutils;

/// Build `build/<arch>/initfs.img` from the profile's programs.
pub(crate) fn run(root: &Path, arch: Arch, profile: &str) -> Result<u8> {
    let path = build(root, arch, profile)?;
    println!("{}", path.display());
    Ok(0)
}

/// Build the initfs and return its path.
pub(crate) fn build(root: &Path, arch: Arch, profile_name: &str) -> Result<PathBuf> {
    let profile = Profile::load(root, profile_name)?;
    ensure_uutils(root, arch, &profile)?;
    let workspace = Workspace::load(root)?;
    let packages: Vec<String> = profile
        .initfs
        .programs
        .iter()
        .map(|program| program.package.clone())
        .collect();
    if !packages.is_empty() {
        let code = build::run(
            &workspace,
            Component::User,
            arch,
            release_user_programs_from_env(std::env::var("RAVEN_USER_RELEASE").ok().as_deref()),
            &packages,
        )?;
        if code != 0 {
            bail!("user program build failed with exit code {code}");
        }
    }

    let mut builder = InitfsBuilder::new();
    for program in &profile.initfs.programs {
        let path = build::artifact_path(root, arch, Component::User, &program.package);
        let data = fs::read(&path).with_context(|| {
            format!(
                "cannot read built user program {} (package `{}` missing from the user build?)",
                path.display(),
                program.package
            )
        })?;
        builder.add_file(&program.path, &data, 0o755)?;
    }
    for file in &profile.initfs.files {
        let data = fs::read(root.join(&file.src)).with_context(|| {
            format!(
                "cannot read profile file {} for initfs path {}",
                file.src, file.path
            )
        })?;
        builder.add_file(&file.path, &data, file.mode)?;
    }
    let image = builder.build();
    let output = root.join("build").join(arch.as_str()).join("initfs.img");
    if let Some(parent) = output.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("cannot create {}", parent.display()))?;
    }
    fs::write(&output, image).with_context(|| format!("cannot write {}", output.display()))?;
    Ok(output)
}

/// Select release user programs for a manually requested image build.
///
/// This is deliberately opt-in: normal images retain their debug user programs, while the K63
/// regression matrix can exercise the release user binary in the same image path.
fn release_user_programs_from_env(value: Option<&str>) -> bool {
    matches!(value, Some("1"))
}

/// Build the uutils slice once when the resolved profile stages a missing uutils binary.
///
/// The profile's `generated = true` entries are not validated by `Profile::load`, so
/// `make image`/`cargo xtask image` can reach the file copy with a clean `build/<arch>/uutils`.
/// The slice build is the same one `cargo xtask build uutils` runs; it is skipped when every
/// referenced staging file already exists so normal image builds stay fast.
pub(crate) fn ensure_uutils(root: &Path, arch: Arch, profile: &Profile) -> Result<()> {
    let exists = |source: &str| root.join(source).is_file();
    let missing = uutils_missing(profile, arch, exists);
    if missing.is_empty() {
        return Ok(());
    }
    println!(
        "==> uutils {}: building missing staging slice ({})",
        arch.as_str(),
        missing.join(", ")
    );
    let code = uutils::run(root, &["--arch".to_string(), arch.as_str().to_string()])?;
    if code != 0 {
        bail!("uutils staging build failed with exit code {code}");
    }
    let exists = |source: &str| root.join(source).is_file();
    let missing = uutils_missing(profile, arch, exists);
    if !missing.is_empty() {
        bail!(
            "uutils build did not produce the staged files: {}",
            missing.join(", ")
        );
    }
    Ok(())
}

/// The uutils name in a profile source such as `build/x86_64/uutils/ls`.
fn uutils_source_name(arch: Arch, source: &str) -> Option<&str> {
    let prefix = format!("build/{}/uutils/", arch.as_str());
    let name = source.strip_prefix(&prefix)?;
    if name.is_empty() || name.contains('/') {
        None
    } else {
        Some(name)
    }
}

/// Names whose staged file is missing, deduplicated in profile order.
fn uutils_missing(profile: &Profile, arch: Arch, exists: impl Fn(&str) -> bool) -> Vec<String> {
    let mut missing = Vec::new();
    for file in &profile.initfs.files {
        let Some(name) = uutils_source_name(arch, &file.src) else {
            continue;
        };
        if exists(&file.src) || missing.iter().any(|seen| seen == name) {
            continue;
        }
        missing.push(name.to_string());
    }
    missing
}

#[cfg(test)]
#[path = "initfs_ut.rs"]
mod tests;
