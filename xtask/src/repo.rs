// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Workspace discovery and package classification.
//!
//! Cargo metadata is the source of truth for the workspace member list. Each member's manifest is
//! read for its package name and the optional `[package.metadata.raven] target` marker. The merged
//! kernel predates that marker and cannot be edited by this ticket, so the known component layout
//! (`kernel/`, `boot/loader/`, userland program directories, `tests/fixtures/`) is used as a
//! fallback.

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, bail};
use serde::Deserialize;

use crate::build::Component;

/// The target class declared by `[package.metadata.raven] target`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TargetKind {
    /// A crate built for a bare (`*-unknown-none`) target.
    Bare,
    /// A crate built for a UEFI target.
    Uefi,
}

impl TargetKind {
    fn parse(value: &str) -> Result<Self> {
        match value {
            "none" => Ok(Self::Bare),
            "uefi" => Ok(Self::Uefi),
            other => bail!("unsupported [package.metadata.raven] target `{other}`"),
        }
    }
}

/// One workspace member.
#[derive(Clone, Debug)]
pub(crate) struct Package {
    pub(crate) name: String,
    pub(crate) rel_dir: String,
    pub(crate) target: Option<TargetKind>,
    pub(crate) component: Option<Component>,
}

/// The repository root and its workspace members.
#[derive(Debug)]
pub(crate) struct Workspace {
    pub(crate) root: PathBuf,
    pub(crate) packages: Vec<Package>,
}

impl Workspace {
    /// Discover the workspace from `cargo metadata`, then read the member manifests.
    pub(crate) fn load(root: &Path) -> Result<Self> {
        let output = Command::new("cargo")
            .args(["metadata", "--no-deps", "--format-version", "1"])
            .current_dir(root)
            .output()
            .context("cannot run `cargo metadata`")?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            bail!("cargo metadata failed: {}", stderr.trim());
        }
        let json =
            String::from_utf8(output.stdout).context("cargo metadata output is not UTF-8")?;
        let mut packages = Vec::new();
        for manifest in manifest_paths(&json) {
            let manifest = PathBuf::from(manifest);
            let parent = manifest
                .parent()
                .with_context(|| format!("manifest has no parent: {}", manifest.display()))?;
            let rel_dir = relative_slash_path(root, parent)?;
            let text = fs::read_to_string(&manifest)
                .with_context(|| format!("cannot read {}", manifest.display()))?;
            packages.push(parse_manifest(&text, &rel_dir)?);
        }
        packages.sort_by(|left, right| left.rel_dir.cmp(&right.rel_dir));
        Ok(Self {
            root: root.to_path_buf(),
            packages,
        })
    }

    pub(crate) fn package(&self, rel_dir: &str) -> Option<&Package> {
        self.packages
            .iter()
            .find(|package| package.rel_dir == rel_dir)
    }

    pub(crate) fn kernel(&self) -> Option<&Package> {
        self.package("kernel")
    }

    pub(crate) fn loader(&self) -> Option<&Package> {
        self.package("boot/loader")
    }

    pub(crate) fn user_packages(&self) -> Vec<&Package> {
        self.packages
            .iter()
            .filter(|package| package.component == Some(Component::User))
            .collect()
    }

    /// Names to pass to `cargo clippy --exclude` so host clippy never sees a target-only crate.
    pub(crate) fn target_only_names(&self) -> Vec<String> {
        let names: BTreeSet<&str> = self
            .packages
            .iter()
            .filter(|package| package.target.is_some())
            .map(|package| package.name.as_str())
            .collect();
        names.into_iter().map(str::to_string).collect()
    }
}

/// Extract the `manifest_path` values from a `cargo metadata --no-deps` document.
///
/// Only workspace members carry a `manifest_path` in that document; dependency names are plain
/// strings in the package's `dependencies` array. The extraction is deliberately limited to this
/// one stable JSON field so xtask does not need a JSON dependency.
pub(crate) fn manifest_paths(json: &str) -> Vec<String> {
    const KEY: &str = "\"manifest_path\":\"";
    let mut paths = Vec::new();
    let mut rest = json;
    while let Some(start) = rest.find(KEY) {
        rest = &rest[start + KEY.len()..];
        let Some(end) = rest.find('"') else {
            break;
        };
        let raw = &rest[..end];
        paths.push(json_unescape(raw));
        rest = &rest[end + 1..];
    }
    paths
}

fn json_unescape(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    let mut chars = raw.chars();
    while let Some(ch) = chars.next() {
        if ch != '\\' {
            out.push(ch);
            continue;
        }
        match chars.next() {
            Some('"') => out.push('"'),
            Some('\\') => out.push('\\'),
            Some('/') => out.push('/'),
            Some('n') => out.push('\n'),
            Some('t') => out.push('\t'),
            Some('r') => out.push('\r'),
            Some(other) => {
                out.push('\\');
                out.push(other);
            }
            None => out.push('\\'),
        }
    }
    out
}

/// Find the repository root from any subdirectory using git.
pub(crate) fn find_root(start: &Path) -> Result<PathBuf> {
    let output = Command::new("git")
        .args(["rev-parse", "--show-toplevel"])
        .current_dir(start)
        .output()
        .with_context(|| format!("cannot run `git rev-parse` in {}", start.display()))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("not inside a git repository: {}", stderr.trim());
    }
    let path = String::from_utf8(output.stdout).context("git output is not UTF-8")?;
    let path = path.trim();
    if path.is_empty() {
        bail!("git returned an empty repository root");
    }
    Ok(PathBuf::from(path))
}

/// Commit metadata used for reproducible builds.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct HeadInfo {
    pub(crate) epoch: i64,
    pub(crate) short: String,
    pub(crate) dirty: bool,
}

/// Read `HEAD`'s timestamp, short hash and worktree cleanliness.
pub(crate) fn head_info(root: &Path) -> Result<HeadInfo> {
    let epoch = git_output(root, &["log", "-1", "--format=%ct"])?;
    let epoch: i64 = epoch
        .parse()
        .with_context(|| format!("cannot parse commit timestamp `{epoch}`"))?;
    let short = git_output(root, &["rev-parse", "--short", "HEAD"])?;
    if short.is_empty() {
        bail!("git returned an empty short hash");
    }
    let status = git_output(root, &["status", "--porcelain"])?;
    Ok(HeadInfo {
        epoch,
        short,
        dirty: !status.is_empty(),
    })
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

fn relative_slash_path(root: &Path, path: &Path) -> Result<String> {
    let relative = path
        .strip_prefix(root)
        .with_context(|| format!("{} is outside {}", path.display(), root.display()))?;
    Ok(relative
        .components()
        .map(|component| component.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/"))
}

#[derive(Deserialize)]
struct Manifest {
    package: ManifestPackage,
}

#[derive(Deserialize)]
struct ManifestPackage {
    name: String,
    metadata: Option<ManifestMetadata>,
}

#[derive(Deserialize)]
struct ManifestMetadata {
    raven: Option<RavenMetadata>,
}

#[derive(Deserialize)]
struct RavenMetadata {
    target: Option<String>,
}

fn parse_manifest(text: &str, rel_dir: &str) -> Result<Package> {
    let manifest: Manifest =
        toml::from_str(text).with_context(|| format!("cannot parse manifest for `{rel_dir}`"))?;
    let declared = manifest
        .package
        .metadata
        .and_then(|metadata| metadata.raven)
        .and_then(|raven| raven.target);
    let mut target = match declared {
        Some(value) => Some(TargetKind::parse(&value)?),
        None => None,
    };
    let component = classify_component(rel_dir);
    if target.is_none() {
        target = fallback_target(rel_dir, component);
    }
    Ok(Package {
        name: manifest.package.name,
        rel_dir: rel_dir.to_string(),
        target,
        component,
    })
}

fn classify_component(rel_dir: &str) -> Option<Component> {
    if rel_dir == "kernel" {
        return Some(Component::Kernel);
    }
    if rel_dir == "boot/loader" {
        return Some(Component::Loader);
    }
    if is_user_dir(rel_dir) {
        return Some(Component::User);
    }
    None
}

fn fallback_target(rel_dir: &str, component: Option<Component>) -> Option<TargetKind> {
    match component {
        Some(Component::Kernel) | Some(Component::User) => Some(TargetKind::Bare),
        Some(Component::Loader) => Some(TargetKind::Uefi),
        None if is_fixture_dir(rel_dir) => Some(TargetKind::Bare),
        None => None,
    }
}

fn is_user_dir(rel_dir: &str) -> bool {
    if rel_dir == "userland/init" {
        return true;
    }
    ["userland/bin/", "userland/servers/", "userland/drivers/"]
        .iter()
        .any(|prefix| {
            rel_dir
                .strip_prefix(prefix)
                .is_some_and(|rest| !rest.is_empty() && !rest.contains('/'))
        })
}

fn is_fixture_dir(rel_dir: &str) -> bool {
    rel_dir
        .strip_prefix("tests/fixtures/")
        .is_some_and(|rest| !rest.is_empty() && !rest.contains('/'))
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"{
        "packages": [
            {"name":"raven-abi","manifest_path":"/repo/crates/raven-abi/Cargo.toml"},
            {"name":"raven-kernel","manifest_path":"/repo/kernel/Cargo.toml"},
            {"name":"with\\escape","manifest_path":"/repo/crates/with\\escape/Cargo.toml"}
        ],
        "workspace_members": []
    }"#;

    #[test]
    fn manifest_paths_are_extracted() {
        assert_eq!(
            manifest_paths(SAMPLE),
            vec![
                "/repo/crates/raven-abi/Cargo.toml".to_string(),
                "/repo/kernel/Cargo.toml".to_string(),
                "/repo/crates/with\\escape/Cargo.toml".to_string(),
            ]
        );
    }

    #[test]
    fn manifest_without_metadata_is_classified_by_layout() {
        let package = parse_manifest("[package]\nname = \"raven-kernel\"\n", "kernel").unwrap();
        assert_eq!(package.name, "raven-kernel");
        assert_eq!(package.target, Some(TargetKind::Bare));
        assert_eq!(package.component, Some(Component::Kernel));
    }

    #[test]
    fn metadata_target_none_is_parsed() {
        let text = "[package]\nname = \"raven-mem\"\n[package.metadata.raven]\ntarget = \"none\"\n";
        let package = parse_manifest(text, "crates/raven-mem").unwrap();
        assert_eq!(package.target, Some(TargetKind::Bare));
        assert_eq!(package.component, None);
    }

    #[test]
    fn metadata_target_uefi_is_parsed() {
        let text =
            "[package]\nname = \"raven-loader\"\n[package.metadata.raven]\ntarget = \"uefi\"\n";
        let package = parse_manifest(text, "boot/loader").unwrap();
        assert_eq!(package.target, Some(TargetKind::Uefi));
        assert_eq!(package.component, Some(Component::Loader));
    }

    #[test]
    fn unknown_metadata_target_is_rejected() {
        let text = "[package]\nname = \"x\"\n[package.metadata.raven]\ntarget = \"host\"\n";
        let error = parse_manifest(text, "crates/x").unwrap_err();
        assert!(error.to_string().contains("unsupported"));
    }

    #[test]
    fn host_crate_has_no_target_marker() {
        let package = parse_manifest(
            "[package]\nname = \"raven-provenance\"\n",
            "crates/raven-provenance",
        )
        .unwrap();
        assert_eq!(package.target, None);
        assert_eq!(package.component, None);
    }

    #[test]
    fn userland_program_is_a_user_component() {
        let package =
            parse_manifest("[package]\nname = \"raven-init\"\n", "userland/init").unwrap();
        assert_eq!(package.target, Some(TargetKind::Bare));
        assert_eq!(package.component, Some(Component::User));
    }

    #[test]
    fn fixture_is_target_only() {
        let package = parse_manifest(
            "[package]\nname = \"raven-testkernel\"\n",
            "tests/fixtures/testkernel",
        )
        .unwrap();
        assert_eq!(package.target, Some(TargetKind::Bare));
        assert_eq!(package.component, None);
    }

    #[test]
    fn nested_userland_directory_is_not_a_program() {
        assert!(!is_user_dir("userland/bin/tool/src"));
        assert!(!is_user_dir("userland/rt"));
        assert!(is_user_dir("userland/bin/tool"));
        assert!(is_user_dir("userland/init"));
    }
}
