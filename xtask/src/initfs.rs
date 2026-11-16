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
    ensure_prerequisites(root, arch, &profile)?;
    let workspace = Workspace::load(root)?;
    let packages: Vec<String> = profile
        .initfs
        .programs
        .iter()
        .map(|program| program.package.clone())
        .collect();
    if !packages.is_empty() {
        let release = release_user_programs(
            profile.initfs.release,
            std::env::var("RAVEN_USER_RELEASE").ok().as_deref(),
        );
        let code = build::run(&workspace, Component::User, arch, release, &packages)?;
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
        let source = source_path(root, arch, &file.src);
        let data = fs::read(&source).with_context(|| {
            format!(
                "cannot read profile file {} for initfs path {} (resolved {})",
                file.src,
                file.path,
                source.display()
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

/// Resolve the user-program profile from the profile's `[initfs] release` switch and
/// `RAVEN_USER_RELEASE`.
///
/// `RAVEN_USER_RELEASE=1` forces the release profile and `=0` forces debug; any other value (or
/// absence) leaves the profile's decision. This keeps the environment override used by
/// `release-gate` and the K63 matrix while `make image PROFILE=release` is self-contained.
fn release_user_programs(profile_release: bool, value: Option<&str>) -> bool {
    match value {
        Some("1") => true,
        Some("0") => false,
        _ => profile_release,
    }
}

/// One prerequisite plan for a resolved profile.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct PrerequisitePlan {
    /// Missing libc archive/crt files, as repo-relative paths.
    pub(crate) libc: Vec<String>,
    /// Whether brush is staged and its arch-correct binary is missing.
    pub(crate) brush: bool,
    /// Missing uutils staging names.
    pub(crate) uutils: Vec<String>,
}

/// The libc outputs every userland C/std build needs.
const LIBC_OUTPUTS: [&str; 4] = ["libc.a", "crt1.o", "crti.o", "crtn.o"];

fn libc_outputs(arch: Arch) -> Vec<String> {
    LIBC_OUTPUTS
        .iter()
        .map(|name| format!("build/{}/{}", arch.as_str(), name))
        .collect()
}

fn libc_missing(arch: Arch, exists: impl Fn(&str) -> bool) -> Vec<String> {
    libc_outputs(arch)
        .into_iter()
        .filter(|path| !exists(path))
        .collect()
}

fn brush_path(arch: Arch) -> String {
    crate::brush::staged_for(arch)
}

fn is_brush_source(source: &str, arch: Arch) -> bool {
    source == "build/brush" || source == brush_path(arch)
}

fn brush_requested(profile: &Profile, arch: Arch) -> bool {
    profile
        .initfs
        .files
        .iter()
        .any(|file| is_brush_source(&file.src, arch))
}

fn brush_missing(profile: &Profile, arch: Arch, exists: impl Fn(&str) -> bool) -> bool {
    brush_requested(profile, arch) && !exists(&brush_path(arch))
}

fn uutils_requested(profile: &Profile, arch: Arch) -> bool {
    profile
        .initfs
        .files
        .iter()
        .any(|file| uutils_source_name(arch, &file.src).is_some())
}

/// Decide which clean-tree prerequisites the profile needs.
///
/// The libc pack is only a prerequisite when the profile stages a libc consumer (brush or
/// uutils); a base image with neither does not pay for it.
pub(crate) fn prerequisite_plan(
    profile: &Profile,
    arch: Arch,
    exists: impl Fn(&str) -> bool,
) -> PrerequisitePlan {
    let needs_libc = brush_requested(profile, arch) || uutils_requested(profile, arch);
    PrerequisitePlan {
        libc: if needs_libc {
            libc_missing(arch, &exists)
        } else {
            Vec::new()
        },
        brush: brush_missing(profile, arch, &exists),
        uutils: uutils_missing(profile, arch, &exists),
    }
}

/// Ensure the libc pack, brush and uutils prerequisites exist before packing an initfs.
///
/// Each step runs at most once per call and only when its outputs are missing. libc is checked
/// first because brush and uutils link against its archive and crt objects.
pub(crate) fn ensure_prerequisites(root: &Path, arch: Arch, profile: &Profile) -> Result<()> {
    let plan = prerequisite_plan(profile, arch, |source| root.join(source).is_file());
    if !plan.libc.is_empty() {
        ensure_libc(root, arch)?;
    }
    if plan.brush {
        ensure_brush(root, arch, profile)?;
    }
    if !plan.uutils.is_empty() {
        ensure_uutils(root, arch, profile)?;
    }
    ensure_fixtures(root, profile)?;
    Ok(())
}

/// Build the fixture files a profile stages from `build/fixtures/<name>/`.
///
/// The Pi 4 DTB is decoded by `tests/fixtures/pi4-release/build.sh`; a clean checkout has no
/// `build/` tree. Run each fixture's build script once when a staged file is missing, then let
/// the normal copy path verify the result.
fn ensure_fixtures(root: &Path, profile: &Profile) -> Result<()> {
    let missing = missing_fixtures(profile, |source| root.join(source).is_file());
    for name in missing {
        let script = root.join("tests/fixtures").join(&name).join("build.sh");
        if !script.is_file() {
            bail!(
                "profile stages build/fixtures/{name}/ but {} is missing",
                script.display()
            );
        }
        println!("==> fixtures: building {name} ({})", script.display());
        let status = std::process::Command::new("sh")
            .arg(&script)
            .current_dir(root)
            .status()
            .with_context(|| format!("cannot run {}", script.display()))?;
        if !status.success() {
            bail!("fixture build {} failed ({status})", script.display());
        }
    }
    Ok(())
}

/// Fixture directory names whose staged files are missing, in profile order.
fn missing_fixtures<F>(profile: &Profile, exists: F) -> Vec<String>
where
    F: Fn(&str) -> bool,
{
    let mut missing = Vec::new();
    for file in &profile.initfs.files {
        let Some(rest) = file.src.strip_prefix("build/fixtures/") else {
            continue;
        };
        let Some(name) = rest.split('/').next() else {
            continue;
        };
        if name.is_empty() || exists(&file.src) || missing.iter().any(|entry| entry == name) {
            continue;
        }
        missing.push(name.to_string());
    }
    missing
}

/// Run the libc pack when the arch archive or a crt object is missing.
pub(crate) fn ensure_libc(root: &Path, arch: Arch) -> Result<()> {
    let missing = libc_missing(arch, |source| root.join(source).is_file());
    if missing.is_empty() {
        return Ok(());
    }
    println!(
        "==> libc {}: packing missing prerequisites ({})",
        arch.as_str(),
        missing.join(", ")
    );
    let code = crate::libc_pack::run(root, &["--arch".to_string(), arch.as_str().to_string()])?;
    if code != 0 {
        bail!("libc-pack failed with exit code {code}");
    }
    let missing = libc_missing(arch, |source| root.join(source).is_file());
    if !missing.is_empty() {
        bail!("libc-pack did not produce: {}", missing.join(", "));
    }
    Ok(())
}

/// Build brush for the resolved arch when a profile stages it and the staged binary is missing.
pub(crate) fn ensure_brush(root: &Path, arch: Arch, profile: &Profile) -> Result<()> {
    if !brush_missing(profile, arch, |source| root.join(source).is_file()) {
        return Ok(());
    }
    let path = brush_path(arch);
    println!(
        "==> brush {}: building missing staging binary ({path})",
        arch.as_str()
    );
    let code = crate::brush::build_for_arch(root, arch)?;
    if code != 0 {
        bail!("brush build failed with exit code {code}");
    }
    if brush_missing(profile, arch, |source| root.join(source).is_file()) {
        bail!("brush build did not produce {path}");
    }
    Ok(())
}

/// Resolve a profile source; the legacy mutable `build/brush` maps to the arch-correct path.
pub(crate) fn source_path(root: &Path, arch: Arch, source: &str) -> PathBuf {
    if source == "build/brush" {
        root.join(brush_path(arch))
    } else {
        root.join(source)
    }
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
