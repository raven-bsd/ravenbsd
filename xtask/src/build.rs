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
/// Whether `Component::User` has a dynamic (PIE) link mode for this arch.
///
/// K97 wired the Rust PIE path for x86_64; K97a extends it to aarch64. Other arches keep the
/// static linker script until their loader/libc start ABI is ready.
pub(crate) fn dynamic_supported(arch: Arch) -> bool {
    matches!(arch, Arch::X86_64 | Arch::Aarch64)
}

/// `user_linker` is passed only when the linker script exists; K06 creates it later.
pub(crate) fn rustflags_for(
    component: Component,
    arch: Arch,
    root: &Path,
    user_linker: Option<&Path>,
    dynamic: bool,
) -> Vec<String> {
    let mut flags = Vec::new();
    match component {
        Component::Kernel => {
            flags.extend(["-C".to_string(), "relocation-model=static".to_string()]);
            flags.extend(["-C".to_string(), "force-frame-pointers=yes".to_string()]);
        }
        Component::User if dynamic && dynamic_supported(arch) => {
            let wrapper = root.join("userland/linker/rust-lld-dynamic.sh");
            // `small` is the x86_64 user-program model K06 established; aarch64's default is
            // already the small model, so leaving it unset keeps its static flag set unchanged.
            if arch == Arch::X86_64 {
                flags.extend(["-C".to_string(), "code-model=small".to_string()]);
            }
            flags.extend(["-C".to_string(), "relocation-model=pic".to_string()]);
            flags.extend(["-C".to_string(), "force-frame-pointers=yes".to_string()]);
            flags.extend(["-C".to_string(), format!("linker={}", wrapper.display())]);
            for arg in [
                "-pie",
                "-Bdynamic",
                "-dynamic-linker",
                "/libexec/ld-elf.so.1",
                "--image-base=0x400000",
                "-z",
                "max-page-size=4096",
                "-z",
                "common-page-size=4096",
            ] {
                flags.push("-C".to_string());
                flags.push(format!("link-arg={arg}"));
            }
            // The precompiled aarch64 `libcore` is not PIC: its anonymous rodata carries
            // `R_AARCH64_ABS64` records. lld accepts those in a PIE only with `notext`, which
            // leaves `DF_TEXTREL` set. The loader rejects text relocations, so this is a build
            // proof only until a PIC libcore or an equivalent build-side fix lands (K97a report).
            if arch == Arch::Aarch64 {
                flags.push("-C".to_string());
                flags.push("link-arg=-z".to_string());
                flags.push("-C".to_string());
                flags.push("link-arg=notext".to_string());
            }
            for path in ["sysroot/lib", "sysroot/usr/lib"] {
                flags.push("-C".to_string());
                flags.push(format!(
                    "link-arg=-L{}",
                    root.join("build").join(arch.as_str()).join(path).display()
                ));
            }
            for arg in ["--allow-shlib-undefined", "--no-as-needed", "-lc"] {
                flags.push("-C".to_string());
                flags.push(format!("link-arg={arg}"));
            }
            flags.extend(["--cfg".to_string(), "sha2_backend=\"soft\"".to_string()]);
        }
        Component::User => {
            if arch == Arch::X86_64 {
                flags.extend(["-C".to_string(), "code-model=small".to_string()]);
            }
            flags.extend(["-C".to_string(), "relocation-model=static".to_string()]);
            flags.extend(["-C".to_string(), "force-frame-pointers=yes".to_string()]);
            // sha2 0.11.1 currently hits an LLVM codegen bug on x86_64-unknown-none; the
            // portable backend produces the same digests and lets raven-auth/users link.
            flags.extend(["--cfg".to_string(), "sha2_backend=\"soft\"".to_string()]);
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
    dynamic: bool,
) -> String {
    rustflags_for(component, arch, root, user_linker, dynamic).join(" ")
}

/// A non-default code model for a component/target (CONTRACTS C2), if the target uses one.
///
/// Only user programs on x86-64 override the target's default code model today. Such a target
/// ships a precompiled `core` built with the default code model, so LTO must not merge the two.
pub(crate) fn code_model_for(component: Component, arch: Arch) -> Option<&'static str> {
    match (component, arch) {
        (Component::User, Arch::X86_64) => Some("small"),
        _ => None,
    }
}

/// The strongest release LTO that links for a component/target (DR-22).
///
/// Kernel, loader and host crates keep the workspace `lto = "fat"`. User programs on x86-64 use a
/// non-default code model while `core` is precompiled with the target default; fat and thin LTO
/// both fail on the conflicting `Code Model` module flag, so the dynamic fallback is `false`.
/// Revisit this mapping if a future toolchain or target links with thin LTO.
pub(crate) fn release_lto_for(component: Component, arch: Arch) -> &'static str {
    if code_model_for(component, arch).is_some() {
        "false"
    } else {
        "fat"
    }
}

/// The `lto=… (code-model=…)` fragment logged for a release build.
pub(crate) fn release_summary(component: Component, arch: Arch) -> String {
    match code_model_for(component, arch) {
        Some(model) => format!(
            "lto={} (code-model={model})",
            release_lto_for(component, arch)
        ),
        None => format!("lto={}", release_lto_for(component, arch)),
    }
}

/// Caller `CARGO_PROFILE_*` keys; xtask drops them so builds are reproducible.
fn caller_profile_vars<I>(vars: I) -> Vec<String>
where
    I: IntoIterator<Item = (String, String)>,
{
    let mut keys: Vec<String> = vars
        .into_iter()
        .map(|(key, _)| key)
        .filter(|key| key.starts_with("CARGO_PROFILE_"))
        .collect();
    keys.sort();
    keys
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
    dynamic: bool,
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
    let linker = if dynamic {
        None
    } else {
        linker.is_file().then_some(linker)
    };
    let flags = rustflags_string(component, arch, &workspace.root, linker.as_deref(), dynamic);
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
    // A caller's cargo profile overrides must not change a component build: drop them all and
    // set the release LTO explicitly (DR-22).
    for key in caller_profile_vars(std::env::vars()) {
        spec = spec.env_remove(key);
    }
    if release {
        spec = spec.env(
            "CARGO_PROFILE_RELEASE_LTO",
            release_lto_for(component, arch),
        );
    }

    let details = if release {
        format!(", release, {}", release_summary(component, arch))
    } else {
        String::new()
    };
    println!(
        "==> build {} {} ({}{})",
        component.as_str(),
        arch.as_str(),
        target,
        details
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
                let binary = user_binary(workspace, package)?;
                copy_artifact(
                    &artifact_dir.join(&binary),
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

/// Cargo binary name for a user package.
///
/// The package name is not always the binary name (`raven-getty` builds `getty`), so the
/// manifest's first `[[bin]]` name wins when present. A package without an explicit binary uses
/// its package name, like cargo does.
fn binary_name(manifest: &str, package: &str) -> Result<String> {
    let value: toml::Value = toml::from_str(manifest)
        .with_context(|| format!("cannot parse the manifest of `{package}`"))?;
    let name = value
        .get("bin")
        .and_then(|bins| bins.as_array())
        .and_then(|bins| bins.first())
        .and_then(|bin| bin.get("name"))
        .and_then(|name| name.as_str());
    Ok(name.unwrap_or(package).to_string())
}

/// Read the manifest of a user package and return its binary name.
fn user_binary(workspace: &Workspace, package: &str) -> Result<String> {
    let entry = workspace
        .packages
        .iter()
        .find(|entry| entry.name == package)
        .with_context(|| format!("user package `{package}` is not in the workspace"))?;
    let manifest = workspace.root.join(&entry.rel_dir).join("Cargo.toml");
    let text = fs::read_to_string(&manifest)
        .with_context(|| format!("cannot read {}", manifest.display()))?;
    binary_name(&text, package)
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
#[path = "build_ut.rs"]
mod tests;
