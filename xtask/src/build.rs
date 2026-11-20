// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Component builds: target triples, the CONTRACTS C2 flag table, reproducible build ids and
//! artifact copies.

use anyhow::{Context, Result, bail};
use std::fs;
use std::path::{Path, PathBuf};

use crate::process::{CommandSpec, run as run_command};
use crate::profiles::Profile;
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
            // The float plug-in is an optional member (R-13/M2): a dynamic program that uses
            // `%f`/`strfrom*` must pull the strong `__rl_fmt_float_impl`, or the libc hook keeps
            // the `*float*` dummy. A weak reference cannot pull it from `libc.so.1` by itself.
            for arg in ["-u", "__rl_fmt_float_impl"] {
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

/// True when the image profile is a release artifact whose kernel must carry the clean HEAD id.
///
/// The profile name is matched explicitly: `pi4-release` does not enable `[initfs] release`, so
/// the release flag cannot be the trigger. Debug and test profiles keep their current behavior.
pub(crate) fn is_release_image_profile(arch: Arch, profile_name: &str) -> bool {
    matches!(
        (arch, profile_name),
        (Arch::X86_64, "release") | (Arch::Aarch64, "pi4-release")
    )
}

/// True when this image must embed a release kernel and loader.
///
/// The release image names are release by default, and any profile can opt in with
/// `[image] release = true` (the explicit switch for demo and test profiles).
pub(crate) fn image_build_release(arch: Arch, profile: &Profile) -> bool {
    profile.image.release || is_release_image_profile(arch, &profile.name)
}

/// Profile-qualified artifact name for a kernel or loader build.
///
/// A release artifact keeps the historical `kernel.elf`/`loader.efi` names so `make release` and
/// the image path agree; a debug build uses a `-debug` name and never overwrites a release file.
pub(crate) fn artifact_filename(component: Component, release: bool) -> Option<&'static str> {
    match component {
        Component::Kernel => Some(if release {
            "kernel.elf"
        } else {
            "kernel-debug.elf"
        }),
        Component::Loader => Some(if release {
            "loader.efi"
        } else {
            "loader-debug.efi"
        }),
        Component::User => None,
    }
}

/// Profile-qualified artifact path for a kernel or loader.
pub(crate) fn profile_artifact_path(
    root: &Path,
    arch: Arch,
    component: Component,
    release: bool,
) -> PathBuf {
    match artifact_filename(component, release) {
        Some(name) => root.join("build").join(arch.as_str()).join(name),
        None => artifact_path(root, arch, component, ""),
    }
}

/// Sidecar recording which cargo profile produced an artifact.
pub(crate) fn artifact_profile_path(artifact: &Path) -> PathBuf {
    let mut name = artifact.as_os_str().to_os_string();
    name.push(".profile");
    PathBuf::from(name)
}

/// Parse a sidecar stamp: `Some(true)` for release, `Some(false)` for debug.
pub(crate) fn parse_artifact_profile(bytes: &[u8]) -> Option<bool> {
    match std::str::from_utf8(bytes).ok()?.trim() {
        "release" => Some(true),
        "debug" => Some(false),
        _ => None,
    }
}

/// Read an artifact's profile stamp.
pub(crate) fn read_artifact_profile(artifact: &Path) -> Option<bool> {
    fs::read(artifact_profile_path(artifact))
        .ok()
        .and_then(|bytes| parse_artifact_profile(&bytes))
}

/// Record the cargo profile next to a freshly built artifact.
pub(crate) fn write_artifact_profile(artifact: &Path, release: bool) -> Result<()> {
    let path = artifact_profile_path(artifact);
    let text = if release { "release\n" } else { "debug\n" };
    fs::write(&path, text).with_context(|| format!("cannot write {}", path.display()))
}

/// Reject an artifact whose sidecar does not name the requested profile.
pub(crate) fn verify_artifact_profile(artifact: &Path, release: bool) -> Result<()> {
    let expected = if release { "release" } else { "debug" };
    match read_artifact_profile(artifact) {
        Some(actual) if actual == release => Ok(()),
        Some(_) => bail!(
            "{} was built with the {} profile; the image requested {expected}",
            artifact.display(),
            if release { "debug" } else { "release" }
        ),
        None => bail!(
            "{} has no build-profile stamp; rebuild it with cargo xtask build",
            artifact.display()
        ),
    }
}

/// Copy a kernel or loader into its profile path and stamp it.
///
/// A release build also refreshes the historical artifact name. A debug build writes only the
/// `-debug` name when a release artifact already occupies the historical path, so a debug image
/// never invalidates a release kernel or loader.
fn install_artifact(
    root: &Path,
    arch: Arch,
    component: Component,
    release: bool,
    source: &Path,
) -> Result<PathBuf> {
    let destination = profile_artifact_path(root, arch, component, release);
    copy_artifact(source, &destination)?;
    write_artifact_profile(&destination, release)?;
    let legacy = artifact_path(root, arch, component, "");
    if release {
        if legacy != destination {
            copy_artifact(&destination, &legacy)?;
            write_artifact_profile(&legacy, true)?;
        }
    } else if !legacy.exists() {
        copy_artifact(&destination, &legacy)?;
        write_artifact_profile(&legacy, false)?;
    }
    Ok(destination)
}

/// True when `data` contains `short` as a standalone build-id token.
///
/// The embedded id is `<short>[-dirty][ <date>]`. A longer hash that merely contains `short`
/// (preceded by another hex digit) and a `short-dirty` id are not a clean match.
pub(crate) fn contains_clean_build_id(data: &[u8], short: &str) -> bool {
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
pub(crate) fn contains_dirty_build_id(data: &[u8], short: &str) -> bool {
    let needle = format!("{short}-dirty");
    data.windows(needle.len())
        .any(|window| window == needle.as_bytes())
}

/// Reject a release kernel that does not carry the clean HEAD build id.
pub(crate) fn verify_release_build_id(data: &[u8], short: &str, dirty: bool) -> Result<()> {
    if short.is_empty() || !short.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        bail!("invalid HEAD short id `{short}`");
    }
    if dirty {
        bail!(
            "release image requires a clean worktree, but HEAD is {short}-dirty; commit or stash the changes"
        );
    }
    if contains_dirty_build_id(data, short) {
        bail!("release kernel was built from a dirty tree ({short}-dirty)");
    }
    if !contains_clean_build_id(data, short) {
        bail!(
            "release kernel does not embed the clean HEAD id {short}; the artifact is stale or was built from another commit"
        );
    }
    Ok(())
}

/// Inspect the kernel artifact that the image embeds and require the clean HEAD id.
///
/// The caller passes the file the image builder actually consumes, so a cached copy from
/// another commit or a dirty tree is rejected instead of trusted.
pub(crate) fn verify_release_kernel(root: &Path, kernel: &Path, profile_name: &str) -> Result<()> {
    let head = crate::repo::head_info(root)?;
    let data = fs::read(kernel)
        .with_context(|| format!("cannot read release kernel {}", kernel.display()))?;
    verify_release_build_id(&data, &head.short, head.dirty)
        .with_context(|| format!("{profile_name} image build rejected"))
}

/// Fail before building a release image from a dirty worktree.
pub(crate) fn require_clean_release_head(root: &Path, profile_name: &str) -> Result<()> {
    let head = crate::repo::head_info(root)?;
    if head.dirty {
        bail!(
            "{profile_name} image requires a clean worktree, but HEAD is {}-dirty; commit or stash the changes",
            head.short
        );
    }
    Ok(())
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
            install_artifact(
                &workspace.root,
                arch,
                Component::Kernel,
                release,
                &artifact_dir.join("raven-kernel"),
            )?;
        }
        Component::Loader => {
            install_artifact(
                &workspace.root,
                arch,
                Component::Loader,
                release,
                &artifact_dir.join("raven-loader.efi"),
            )?;
        }
        Component::User => {
            for package in &packages {
                let binary = user_binary(workspace, package)?;
                let destination = artifact_path(&workspace.root, arch, Component::User, package);
                copy_artifact(&artifact_dir.join(&binary), &destination)?;
                if dynamic && arch == Arch::Aarch64 {
                    patch_aarch64_textrel(&destination)?;
                }
            }
        }
    }
    Ok(0)
}

/// Build the Pi 4 kernel with `RAVEN_PI4=1` and the requested cargo profile.
///
/// The generic component build has no `RAVEN_PI4` switch, so the Pi image path uses this entry
/// for both debug and release kernels. The result is copied and stamped like any other artifact.
pub(crate) fn run_pi4_kernel(root: &Path, release: bool) -> Result<PathBuf> {
    let workspace = Workspace::load(root)?;
    let package = workspace
        .kernel()
        .ok_or_else(|| anyhow::anyhow!("component 'kernel' does not exist yet"))?
        .name
        .clone();
    let target = target_for(Component::Kernel, Arch::Aarch64)?;
    let target_dir = root.join("build/cargo/kernel-aarch64-pi4");
    let flags = rustflags_string(Component::Kernel, Arch::Aarch64, root, None, false);
    let head = crate::repo::head_info(root)?;
    let build_id = if head.dirty {
        format!("{}-dirty", head.short)
    } else {
        head.short.clone()
    };
    let mut args = vec![
        "build".to_string(),
        "-p".to_string(),
        package,
        "--target".to_string(),
        target.to_string(),
        "--target-dir".to_string(),
        target_dir.display().to_string(),
    ];
    if release {
        args.push("--release".to_string());
    }
    let mut spec = CommandSpec::new("cargo", args, root)
        .env("RUSTFLAGS", flags)
        .env("RAVEN_PI4", "1")
        .env("SOURCE_DATE_EPOCH", head.epoch.to_string())
        .env("RAVEN_BUILD_ID", build_id)
        .env_remove("CARGO_ENCODED_RUSTFLAGS");
    for key in std::env::vars()
        .map(|(key, _)| key)
        .filter(|key| key.starts_with("CARGO_PROFILE_"))
    {
        spec = spec.env_remove(key);
    }
    if release {
        spec = spec.env(
            "CARGO_PROFILE_RELEASE_LTO",
            release_lto_for(Component::Kernel, Arch::Aarch64),
        );
    }
    println!(
        "==> build kernel aarch64 (pi4, {target}{})",
        if release {
            format!(
                ", release, {}",
                release_summary(Component::Kernel, Arch::Aarch64)
            )
        } else {
            String::new()
        }
    );
    let code = run_command(&spec)?;
    if code != 0 {
        bail!("Pi kernel build failed with exit code {code}");
    }
    let profile_dir = if release { "release" } else { "debug" };
    let source = target_dir
        .join(target)
        .join(profile_dir)
        .join("raven-kernel");
    install_artifact(root, Arch::Aarch64, Component::Kernel, release, &source)
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

/// ELF program-header types and flags used by the aarch64 textrel post-link pass.
const PT_LOAD: u32 = 1;
const PT_DYNAMIC: u32 = 2;
const PF_X: u32 = 1;
const PF_W: u32 = 2;
const PF_R: u32 = 4;
const DT_NULL: i64 = 0;
const DT_RELA: i64 = 7;
const DT_RELASZ: i64 = 8;
const DT_RELAENT: i64 = 9;
const DT_TEXTREL: i64 = 22;
const DT_FLAGS: i64 = 30;
const DF_TEXTREL: u64 = 0x4;
const EM_AARCH64: u16 = 183;

#[derive(Clone, Copy)]
struct ProgramHeader {
    offset: usize,
    kind: u32,
    flags: u32,
    file_offset: u64,
    vaddr: u64,
    filesz: u64,
    memsz: u64,
}

fn read_u16(data: &[u8], offset: usize) -> Option<u16> {
    Some(u16::from_le_bytes(
        data.get(offset..offset.checked_add(2)?)?.try_into().ok()?,
    ))
}

fn read_u32(data: &[u8], offset: usize) -> Option<u32> {
    Some(u32::from_le_bytes(
        data.get(offset..offset.checked_add(4)?)?.try_into().ok()?,
    ))
}

fn read_u64(data: &[u8], offset: usize) -> Option<u64> {
    Some(u64::from_le_bytes(
        data.get(offset..offset.checked_add(8)?)?.try_into().ok()?,
    ))
}

fn read_i64(data: &[u8], offset: usize) -> Option<i64> {
    Some(i64::from_le_bytes(
        data.get(offset..offset.checked_add(8)?)?.try_into().ok()?,
    ))
}

fn write_u32(data: &mut [u8], offset: usize, value: u32) -> Option<()> {
    data.get_mut(offset..offset.checked_add(4)?)?
        .copy_from_slice(&value.to_le_bytes());
    Some(())
}

fn write_u64(data: &mut [u8], offset: usize, value: u64) -> Option<()> {
    data.get_mut(offset..offset.checked_add(8)?)?
        .copy_from_slice(&value.to_le_bytes());
    Some(())
}

fn write_i64(data: &mut [u8], offset: usize, value: i64) -> Option<()> {
    data.get_mut(offset..offset.checked_add(8)?)?
        .copy_from_slice(&value.to_le_bytes());
    Some(())
}

/// Rewrite an aarch64 PIE so the segment holding the non-PIC core's relative relocations is
/// writable, and drop `DT_TEXTREL`/`DF_TEXTREL`.
///
/// The precompiled `aarch64-unknown-none-softfloat` `libcore` is `RelocModel::Static`; linking
/// a PIE needs `-z notext`, which leaves `DF_TEXTREL` and makes the loader reject the object
/// (`DigestError::TextRel`). The relocations are in the read-only data segment, not code, so
/// marking that segment `R|W` (the text segment stays `R|E`) removes the text-relocation
/// condition without weakening the loader's W^X or genuine-textrel checks.
fn patch_aarch64_textrel(path: &Path) -> Result<()> {
    let mut data = fs::read(path).with_context(|| format!("cannot read {}", path.display()))?;
    if data.len() < 64 || data.get(..4) != Some(b"\x7fELF") {
        return Ok(());
    }
    let class = *data.get(4).unwrap_or(&0);
    let encoding = *data.get(5).unwrap_or(&0);
    if class != 2 || encoding != 1 {
        return Ok(());
    }
    let machine = read_u16(&data, 18).unwrap_or(0);
    if machine != EM_AARCH64 {
        return Ok(());
    }
    let phoff = read_u64(&data, 0x20).unwrap_or(0);
    let phentsize = usize::from(read_u16(&data, 0x36).unwrap_or(0));
    let phnum = usize::from(read_u16(&data, 0x38).unwrap_or(0));
    if phentsize < 56 {
        bail!("aarch64 ELF {} has a short program header", path.display());
    }
    let phoff = usize::try_from(phoff).context("program header offset")?;
    let mut headers = Vec::new();
    for index in 0..phnum {
        let offset = phoff
            .checked_add(index.checked_mul(phentsize).context("program header")?)
            .context("program header")?;
        let kind = read_u32(&data, offset).unwrap_or(0);
        let flags = read_u32(&data, offset + 4).unwrap_or(0);
        let file_offset = read_u64(&data, offset + 8).unwrap_or(0);
        let vaddr = read_u64(&data, offset + 16).unwrap_or(0);
        let filesz = read_u64(&data, offset + 32).unwrap_or(0);
        let memsz = read_u64(&data, offset + 40).unwrap_or(0);
        headers.push(ProgramHeader {
            offset,
            kind,
            flags,
            file_offset,
            vaddr,
            filesz,
            memsz,
        });
    }
    let Some(dynamic) = headers.iter().find(|header| header.kind == PT_DYNAMIC) else {
        return Ok(());
    };
    let dynamic_offset = usize::try_from(dynamic.file_offset).context("dynamic offset")?;
    let dynamic_len = usize::try_from(dynamic.filesz).context("dynamic size")?;
    let mut entries = Vec::new();
    let mut cursor = dynamic_offset;
    let end = dynamic_offset
        .checked_add(dynamic_len)
        .context("dynamic range")?;
    loop {
        if cursor.checked_add(16).is_none_or(|next| next > end) {
            bail!(
                "aarch64 ELF {} has an unterminated dynamic table",
                path.display()
            );
        }
        let tag = read_i64(&data, cursor).unwrap_or(0);
        let value = read_u64(&data, cursor + 8).unwrap_or(0);
        entries.push((tag, value));
        cursor += 16;
        if tag == DT_NULL {
            break;
        }
    }
    let textrel = entries.iter().any(|(tag, _)| *tag == DT_TEXTREL)
        || entries
            .iter()
            .any(|(tag, value)| *tag == DT_FLAGS && value & DF_TEXTREL != 0);
    if !textrel {
        return Ok(());
    }
    let mut rela = None;
    let mut relasz = 0u64;
    let mut relaent = 24u64;
    for (tag, value) in &entries {
        match *tag {
            DT_RELA => rela = Some(*value),
            DT_RELASZ => relasz = *value,
            DT_RELAENT => relaent = *value,
            _ => {}
        }
    }
    let Some(rela) = rela else {
        bail!(
            "aarch64 ELF {} has a textrel flag but no DT_RELA",
            path.display()
        );
    };
    if relaent < 24 {
        bail!("aarch64 ELF {} has a short RELA entry", path.display());
    }
    let relaent = usize::try_from(relaent).context("rela entry")?;
    let relasz = usize::try_from(relasz).context("rela size")?;
    let rela_offset = file_offset(&headers, rela)
        .with_context(|| format!("aarch64 ELF {} has no RELA segment", path.display()))?;
    let mut targets = std::collections::BTreeSet::new();
    for index in 0..relasz / relaent {
        let offset = rela_offset
            .checked_add(index.checked_mul(relaent).context("rela entry")?)
            .context("rela entry")?;
        let target = read_u64(&data, offset).unwrap_or(0);
        for header in &headers {
            if header.kind != PT_LOAD
                || target < header.vaddr
                || target >= header.vaddr.saturating_add(header.memsz)
            {
                continue;
            }
            if header.flags & PF_R != 0 && (header.flags & (PF_W | PF_X)) == 0 {
                targets.insert(header.offset);
            }
            break;
        }
    }
    // Some objects (e.g. `raven-uname`) have all RELATIVE slots in `.data.rel.ro`; the textrel
    // flag is then spurious and only the dynamic flags need clearing.
    for offset in &targets {
        let flags = read_u32(&data, offset + 4).unwrap_or(0) | PF_W;
        if write_u32(&mut data, offset + 4, flags).is_none() {
            bail!("cannot set segment flags in {}", path.display());
        }
    }
    let mut rewritten = Vec::new();
    for (tag, value) in entries {
        if tag == DT_TEXTREL {
            continue;
        }
        let value = if tag == DT_FLAGS {
            value & !DF_TEXTREL
        } else {
            value
        };
        rewritten.push((tag, value));
    }
    let needed = rewritten
        .len()
        .checked_add(1)
        .and_then(|count| count.checked_mul(16))
        .context("dynamic table size")?;
    if needed > dynamic_len {
        bail!(
            "aarch64 ELF {} dynamic table is too small after patch",
            path.display()
        );
    }
    let mut write = dynamic_offset;
    for (tag, value) in &rewritten {
        if write_i64(&mut data, write, *tag).is_none()
            || write_u64(&mut data, write + 8, *value).is_none()
        {
            bail!("cannot rewrite dynamic table in {}", path.display());
        }
        write += 16;
    }
    if write_i64(&mut data, write, DT_NULL).is_none()
        || write_u64(&mut data, write + 8, 0).is_none()
    {
        bail!("cannot terminate dynamic table in {}", path.display());
    }
    write += 16;
    while write < end {
        if write_i64(&mut data, write, DT_NULL).is_none()
            || write_u64(&mut data, write + 8, 0).is_none()
        {
            bail!("cannot clear dynamic table tail in {}", path.display());
        }
        write += 16;
    }
    fs::write(path, data).with_context(|| format!("cannot write {}", path.display()))?;
    println!(
        "    patched aarch64 textrel: {} ({} segment(s) made writable)",
        path.display(),
        targets.len()
    );
    Ok(())
}

fn file_offset(headers: &[ProgramHeader], address: u64) -> Option<usize> {
    headers.iter().find_map(|header| {
        let size = if header.kind == PT_LOAD {
            header.memsz
        } else {
            header.filesz
        };
        if header.kind == PT_LOAD
            && address >= header.vaddr
            && address < header.vaddr.saturating_add(size)
        {
            usize::try_from(header.file_offset.saturating_add(address - header.vaddr)).ok()
        } else {
            None
        }
    })
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
