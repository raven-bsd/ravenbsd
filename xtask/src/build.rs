// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Component builds: target triples, the CONTRACTS C2 flag table, reproducible build ids and
//! artifact copies.

use anyhow::{Context, Result, bail};
use std::fs;
use std::path::{Path, PathBuf};

use crate::process::{CommandSpec, run as run_command};
use crate::repo::Workspace;

/// A buildable component.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Component {
    Kernel,
    Loader,
    User,
}

impl Component {
    pub(crate) fn parse(value: &str) -> Option<Self> {
        match value {
            "kernel" => Some(Self::Kernel),
            "loader" => Some(Self::Loader),
            "user" => Some(Self::User),
            _ => None,
        }
    }

    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Kernel => "kernel",
            Self::Loader => "loader",
            Self::User => "user",
        }
    }
}

/// A supported architecture.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Arch {
    X86_64,
    Aarch64,
    Riscv64,
}

impl Arch {
    pub(crate) fn parse(value: &str) -> Option<Self> {
        match value {
            "x86_64" => Some(Self::X86_64),
            "aarch64" => Some(Self::Aarch64),
            "riscv64" => Some(Self::Riscv64),
            _ => None,
        }
    }

    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::X86_64 => "x86_64",
            Self::Aarch64 => "aarch64",
            Self::Riscv64 => "riscv64",
        }
    }
}

/// The Rust target triple for a component and architecture (CONTRACTS C2).
pub(crate) fn target_for(component: Component, arch: Arch) -> Result<&'static str> {
    match (component, arch) {
        (Component::Kernel | Component::User, Arch::X86_64) => Ok("x86_64-unknown-none"),
        (Component::Kernel | Component::User, Arch::Aarch64) => {
            Ok("aarch64-unknown-none-softfloat")
        }
        (Component::Kernel | Component::User, Arch::Riscv64) => Ok("riscv64gc-unknown-none-elf"),
        (Component::Loader, Arch::X86_64) => Ok("x86_64-unknown-uefi"),
        (Component::Loader, Arch::Aarch64) => Ok("aarch64-unknown-uefi"),
        (Component::Loader, Arch::Riscv64) => bail!("loader is not supported for riscv64"),
    }
}

/// The RUSTFLAGS token list for a component (CONTRACTS C2).
///
/// `user_linker` is passed only when the linker script exists; K06 creates it later.
pub(crate) fn rustflags_for(
    component: Component,
    arch: Arch,
    root: &Path,
    user_linker: Option<&Path>,
) -> Vec<String> {
    let mut flags = Vec::new();
    match component {
        Component::Kernel => {
            flags.extend(["-C".to_string(), "relocation-model=static".to_string()]);
            flags.extend(["-C".to_string(), "force-frame-pointers=yes".to_string()]);
        }
        Component::User => {
            if arch == Arch::X86_64 {
                flags.extend(["-C".to_string(), "code-model=small".to_string()]);
            }
            flags.extend(["-C".to_string(), "relocation-model=static".to_string()]);
            flags.extend(["-C".to_string(), "force-frame-pointers=yes".to_string()]);
            if let Some(linker) = user_linker {
                flags.push("-C".to_string());
                flags.push(format!("link-arg=-T{}", linker.display()));
            }
        }
        Component::Loader => {}
    }
    flags.push(format!("--remap-path-prefix={}=/ravenbsd", root.display()));
    flags
}

pub(crate) fn rustflags_string(
    component: Component,
    arch: Arch,
    root: &Path,
    user_linker: Option<&Path>,
) -> String {
    rustflags_for(component, arch, root, user_linker).join(" ")
}

/// The uname build id: `<short>[-dirty] <YYYY-MM-DD>` derived from the commit timestamp.
pub(crate) fn build_id_from(epoch: i64, short: &str, dirty: bool) -> String {
    let days = epoch.div_euclid(86_400);
    let (year, month, day) = civil_from_days(days);
    let dirty = if dirty { "-dirty" } else { "" };
    format!("{short}{dirty} {year:04}-{month:02}-{day:02}")
}

/// Convert days since 1970-01-01 to a civil date (Howard Hinnant's algorithm, proleptic Gregorian).
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let year = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    (
        if month <= 2 { year + 1 } else { year },
        month as u32,
        day as u32,
    )
}

/// Build one component and copy its artifacts into `build/<arch>/`.
pub(crate) fn run(
    workspace: &Workspace,
    component: Component,
    arch: Arch,
    release: bool,
    selected: &[String],
) -> Result<u8> {
    let packages = select_packages(workspace, component, selected)?;
    let target = target_for(component, arch)?;
    let target_dir = workspace.root.join("build/cargo").join(format!(
        "{}-{}",
        component.as_str(),
        arch.as_str()
    ));
    let linker = workspace
        .root
        .join("userland/linker")
        .join(format!("{}.ld", arch.as_str()));
    let linker = linker.is_file().then_some(linker);
    let flags = rustflags_string(component, arch, &workspace.root, linker.as_deref());
    let head = crate::repo::head_info(&workspace.root)?;

    let mut args = vec!["build".to_string()];
    if release {
        args.push("--release".to_string());
    }
    for package in &packages {
        args.push("-p".to_string());
        args.push(package.clone());
    }
    args.extend(["--target".to_string(), target.to_string()]);
    args.extend(["--target-dir".to_string(), target_dir.display().to_string()]);
    let mut spec = CommandSpec::new("cargo", args, &workspace.root)
        .env("RUSTFLAGS", flags)
        .env("SOURCE_DATE_EPOCH", head.epoch.to_string())
        .env(
            "RAVEN_BUILD_ID",
            build_id_from(head.epoch, &head.short, head.dirty),
        );
    spec = spec.env_remove("CARGO_ENCODED_RUSTFLAGS");

    println!(
        "==> build {} {} ({}{})",
        component.as_str(),
        arch.as_str(),
        target,
        if release { ", release" } else { "" }
    );
    let code = run_command(&spec)?;
    if code != 0 {
        eprintln!("xtask: cargo build failed with exit code {code}");
        return Ok(1);
    }

    let profile_dir = if release { "release" } else { "debug" };
    let artifact_dir = target_dir.join(target).join(profile_dir);
    match component {
        Component::Kernel => {
            copy_artifact(
                &artifact_dir.join("raven-kernel"),
                &artifact_path(&workspace.root, arch, Component::Kernel, ""),
            )?;
        }
        Component::Loader => {
            copy_artifact(
                &artifact_dir.join("raven-loader.efi"),
                &artifact_path(&workspace.root, arch, Component::Loader, ""),
            )?;
        }
        Component::User => {
            for package in &packages {
                copy_artifact(
                    &artifact_dir.join(package),
                    &artifact_path(&workspace.root, arch, Component::User, package),
                )?;
            }
        }
    }
    Ok(0)
}

fn select_packages(
    workspace: &Workspace,
    component: Component,
    selected: &[String],
) -> Result<Vec<String>> {
    match component {
        Component::Kernel => workspace
            .kernel()
            .map(|package| vec![package.name.clone()])
            .ok_or_else(|| anyhow::anyhow!("component 'kernel' does not exist yet")),
        Component::Loader => workspace
            .loader()
            .map(|package| vec![package.name.clone()])
            .ok_or_else(|| anyhow::anyhow!("component 'loader' does not exist yet")),
        Component::User => {
            let available = workspace.user_packages();
            if selected.is_empty() {
                if available.is_empty() {
                    bail!("component 'user' does not exist yet");
                }
                return Ok(available
                    .iter()
                    .map(|package| package.name.clone())
                    .collect());
            }
            for name in selected {
                if !available.iter().any(|package| package.name == *name) {
                    bail!("user package '{name}' does not exist");
                }
            }
            Ok(selected.to_vec())
        }
    }
}

fn copy_artifact(source: &Path, destination: &Path) -> Result<()> {
    if !source.is_file() {
        bail!("built artifact not found: {}", source.display());
    }
    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("cannot create {}", parent.display()))?;
    }
    fs::copy(source, destination).with_context(|| {
        format!(
            "cannot copy {} to {}",
            source.display(),
            destination.display()
        )
    })?;
    println!("    {}", destination.display());
    Ok(())
}

/// Path of a copied artifact under `build/<arch>/`.
pub(crate) fn artifact_path(
    root: &Path,
    arch: Arch,
    component: Component,
    package: &str,
) -> PathBuf {
    let out = root.join("build").join(arch.as_str());
    match component {
        Component::Kernel => out.join("kernel.elf"),
        Component::Loader => out.join("loader.efi"),
        Component::User => out.join("user").join(package),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kernel_target_is_unknown_none() {
        assert_eq!(
            target_for(Component::Kernel, Arch::X86_64).unwrap(),
            "x86_64-unknown-none"
        );
        assert_eq!(
            target_for(Component::Kernel, Arch::Aarch64).unwrap(),
            "aarch64-unknown-none-softfloat"
        );
        assert_eq!(
            target_for(Component::Kernel, Arch::Riscv64).unwrap(),
            "riscv64gc-unknown-none-elf"
        );
    }

    #[test]
    fn user_targets_match_kernel_targets() {
        for arch in [Arch::X86_64, Arch::Aarch64, Arch::Riscv64] {
            assert_eq!(
                target_for(Component::User, arch).unwrap(),
                target_for(Component::Kernel, arch).unwrap()
            );
        }
    }

    #[test]
    fn loader_targets_are_uefi() {
        assert_eq!(
            target_for(Component::Loader, Arch::X86_64).unwrap(),
            "x86_64-unknown-uefi"
        );
        assert_eq!(
            target_for(Component::Loader, Arch::Aarch64).unwrap(),
            "aarch64-unknown-uefi"
        );
    }

    #[test]
    fn loader_has_no_riscv64_target() {
        assert!(target_for(Component::Loader, Arch::Riscv64).is_err());
    }

    #[test]
    fn kernel_rustflags_match_c2() {
        let flags = rustflags_for(Component::Kernel, Arch::X86_64, Path::new("/r"), None);
        assert_eq!(
            flags,
            vec![
                "-C",
                "relocation-model=static",
                "-C",
                "force-frame-pointers=yes",
                "--remap-path-prefix=/r=/ravenbsd",
            ]
        );
    }

    #[test]
    fn user_rustflags_include_small_code_model_on_x86_64() {
        let linker = Path::new("/r/userland/linker/x86_64.ld");
        let flags = rustflags_for(Component::User, Arch::X86_64, Path::new("/r"), Some(linker));
        assert_eq!(
            flags,
            vec![
                "-C",
                "code-model=small",
                "-C",
                "relocation-model=static",
                "-C",
                "force-frame-pointers=yes",
                "-C",
                "link-arg=-T/r/userland/linker/x86_64.ld",
                "--remap-path-prefix=/r=/ravenbsd",
            ]
        );
    }

    #[test]
    fn user_rustflags_omit_missing_linker() {
        let flags = rustflags_for(Component::User, Arch::Aarch64, Path::new("/r"), None);
        assert!(!flags.iter().any(|flag| flag.contains("link-arg")));
        assert!(!flags.iter().any(|flag| flag == "code-model=small"));
    }

    #[test]
    fn loader_rustflags_only_remap() {
        let flags = rustflags_for(Component::Loader, Arch::X86_64, Path::new("/r"), None);
        assert_eq!(flags, vec!["--remap-path-prefix=/r=/ravenbsd"]);
    }

    #[test]
    fn build_id_uses_the_commit_date() {
        assert_eq!(build_id_from(0, "abc1234", false), "abc1234 1970-01-01");
        assert_eq!(
            build_id_from(1_609_459_200, "abc1234", true),
            "abc1234-dirty 2021-01-01"
        );
    }

    #[test]
    fn civil_date_handles_leap_day() {
        assert_eq!(civil_from_days(19_782), (2024, 2, 29));
    }

    #[test]
    fn civil_date_handles_epoch() {
        assert_eq!(civil_from_days(0), (1970, 1, 1));
        assert_eq!(civil_from_days(-1), (1969, 12, 31));
    }

    #[test]
    fn artifact_paths_are_stable() {
        let root = Path::new("/repo");
        assert_eq!(
            artifact_path(root, Arch::X86_64, Component::Kernel, ""),
            Path::new("/repo/build/x86_64/kernel.elf")
        );
        assert_eq!(
            artifact_path(root, Arch::Aarch64, Component::Loader, ""),
            Path::new("/repo/build/aarch64/loader.efi")
        );
        assert_eq!(
            artifact_path(root, Arch::Riscv64, Component::User, "raven-init"),
            Path::new("/repo/build/riscv64/user/raven-init")
        );
    }
}
