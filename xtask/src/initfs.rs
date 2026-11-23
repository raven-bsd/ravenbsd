// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Initial filesystem packing and profile-driven user builds.

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, anyhow, bail};
use raven_elf::{Elf, Machine};
use raven_initfs::InitfsBuilder;

use crate::build::{self, Arch, Component};
use crate::profiles::{Profile, Program};
use crate::repo::Workspace;
use crate::uutils;

/// The reproducible initfs epoch.
///
/// `SOURCE_DATE_EPOCH` wins when the caller sets it for an A/B reproducible build; otherwise the
/// commit timestamp is used. The host clock is never consulted. The last fallback is the
/// documented default epoch, so an entry never falls back to 1970.
pub(crate) fn image_epoch(root: &Path) -> u64 {
    if let Some(epoch) = crate::image::source_date_epoch()
        && let Ok(value) = u64::try_from(epoch)
    {
        return value;
    }
    crate::repo::head_info(root)
        .ok()
        .and_then(|head| u64::try_from(head.epoch).ok())
        .unwrap_or(raven_initfs::DEFAULT_EPOCH)
}

/// Shell-visible first-party `/bin` tools that must be statically linked.
///
/// The release/shell userland links `/bin` programs as dynamic PIE, but libc `posix_spawn`
/// refuses an interpreter image through `spawn_image_with_fdmap`; the shell would report a
/// dynamic tool as "command not found". `/sbin` programs are static by path; this list covers
/// the `/bin` tools the login shell runs. `raven-getty` and `login` stay dynamic so the release
/// gate keeps exercising the loader and libc startup path.
const STATIC_SHELL_PACKAGES: &[&str] = &[
    "curl",
    "ed",
    "man",
    "raven-date",
    "raven-hostname",
    "raven-ps",
    "raven-rsh",
    "raven-sleep",
    "raven-svcctl",
    "raven-uname",
    "wget",
];

/// Whether a profile program is built static rather than PIE.
fn program_is_static(program: &Program) -> bool {
    !program.path.starts_with("/bin/") || STATIC_SHELL_PACKAGES.contains(&program.package.as_str())
}

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
        let (static_packages, dynamic_packages): (Vec<String>, Vec<String>) =
            packages.iter().cloned().partition(|package| {
                profile
                    .initfs
                    .programs
                    .iter()
                    .find(|program| &program.package == package)
                    .is_none_or(program_is_static)
            });
        if !static_packages.is_empty() {
            let code = build::run(
                &workspace,
                Component::User,
                arch,
                release,
                &static_packages,
                false,
            )?;
            if code != 0 {
                bail!("user program build failed with exit code {code}");
            }
        }
        if !dynamic_packages.is_empty() {
            let code = build::run(
                &workspace,
                Component::User,
                arch,
                release,
                &dynamic_packages,
                profile.initfs.dynamic,
            )?;
            if code != 0 {
                bail!("dynamic user program build failed with exit code {code}");
            }
        }
    }

    let mut builder = InitfsBuilder::new().with_image_epoch(image_epoch(root));
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
    for (path, source) in dynamic_stage_files(arch, profile.initfs.dynamic) {
        let data = fs::read(root.join(&source)).with_context(|| {
            format!(
                "cannot read dynamic runtime {source} for initfs path {path}; libc-link must stage it"
            )
        })?;
        builder.add_file(path, &data, 0o755)?;
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
        verify_staged_elf(arch, &source, &data)
            .with_context(|| format!("initfs path {}", file.path))?;
        builder.add_file(&file.path, &data, file.mode)?;
    }
    let image = builder.build();
    verify_dynamic_payload(&image, root, arch, profile.initfs.dynamic)?;
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
const DYNAMIC_OUTPUTS: [&str; 2] = ["libc.so.1", "ld-elf.so.1"];

/// The dynamic loader and libc an initfs stages for the `/bin` PIE userland.
///
/// Returns `(destination, source)` pairs. Empty when the profile is static or the arch has no
/// dynamic user mode yet, so an aarch64 shell image never picks up x86_64 loader files.
pub(crate) fn dynamic_stage_files(arch: Arch, dynamic: bool) -> Vec<(&'static str, String)> {
    if !dynamic || !build::dynamic_supported(arch) {
        return Vec::new();
    }
    vec![
        (
            "/lib/libc.so.1",
            format!("build/{}/libc.so.1", arch.as_str()),
        ),
        (
            "/libexec/ld-elf.so.1",
            format!("build/{}/ld-elf.so.1", arch.as_str()),
        ),
    ]
}

fn libc_outputs(arch: Arch, dynamic: bool) -> Vec<String> {
    let mut names: Vec<&str> = LIBC_OUTPUTS.to_vec();
    if dynamic {
        names.extend(DYNAMIC_OUTPUTS);
    }
    names
        .into_iter()
        .map(|name| format!("build/{}/{}", arch.as_str(), name))
        .collect()
}

fn libc_missing(arch: Arch, dynamic: bool, exists: impl Fn(&str) -> bool) -> Vec<String> {
    libc_outputs(arch, dynamic)
        .into_iter()
        .filter(|path| !exists(path))
        .collect()
}

/// True when the libc stamp's source hash matches the current libc tree.
///
/// `libc-pack --verify` uses the same stamp for the archive and sysroot hashes; the image
/// path only needs the source gate, because a matching source hash means the pack was made
/// from this tree.
pub(crate) fn libc_stamp_current(root: &Path, arch: Arch) -> bool {
    let stamp = root
        .join("build")
        .join(arch.as_str())
        .join(crate::libc_pack::STAMP_FILE);
    let Ok(text) = fs::read_to_string(stamp) else {
        return false;
    };
    let Some(sources) = crate::libc_pack::stamp_field(&text, "sources") else {
        return false;
    };
    let Ok(sources) = u64::from_str_radix(sources, 16) else {
        return false;
    };
    crate::libc_pack::source_hash(root).is_ok_and(|hash| hash == sources)
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
    let dynamic = profile.initfs.dynamic && build::dynamic_supported(arch);
    let needs_libc = brush_requested(profile, arch) || uutils_requested(profile, arch) || dynamic;
    PrerequisitePlan {
        libc: if needs_libc {
            libc_missing(arch, dynamic, &exists)
        } else {
            Vec::new()
        },
        brush: brush_missing(profile, arch, &exists),
        uutils: uutils_missing(profile, arch, &exists),
    }
}

/// Staged paths that a release image must not inherit from an older build.
///
/// `build/<arch>/brush`/`uutils` are copied into the image and the dynamic
/// `libc.so.1`/`ld-elf.so.1` pair is linked from the current sources, so those outputs are
/// discarded first: a release image must be reproducible from the current checkout.
///
/// The cargo target caches (`build/cargo/brush-*`, `build/cargo/uutils-*`) are deliberately
/// kept. Cargo fingerprints the feature set and RUSTFLAGS, and the staged tree is rebuilt from
/// the cache, so keeping the cache cannot pack a stale binary; it only avoids paying the full
/// cold compile of both slices on every release image (K548 image wall).
pub(crate) fn release_userspace_paths(root: &Path, arch: Arch) -> Vec<PathBuf> {
    vec![
        root.join(brush_path(arch)),
        root.join("build").join(arch.as_str()).join("uutils"),
        root.join("build").join(arch.as_str()).join("libc.so.1"),
        root.join("build").join(arch.as_str()).join("ld-elf.so.1"),
        crate::libc_link::loader_manifest_path(root, arch.as_str()),
    ]
}

fn purge_release_userspace(root: &Path, arch: Arch) -> Result<()> {
    for path in release_userspace_paths(root, arch) {
        if path.is_dir() {
            fs::remove_dir_all(&path)
                .with_context(|| format!("cannot remove {}", path.display()))?;
        } else if path.exists() {
            fs::remove_file(&path).with_context(|| format!("cannot remove {}", path.display()))?;
        }
    }
    Ok(())
}

/// Ensure the libc pack, brush and uutils prerequisites exist before packing an initfs.
///
/// Each step runs at most once per call and only when its outputs are missing. libc is checked
/// first because brush and uutils link against its archive and crt objects. A release profile
/// discards the staged brush/uutils outputs and the dynamic loader first, then rebuilds them
/// from the cargo target cache; cargo's own fingerprint (features/RUSTFLAGS/sources) decides what
/// recompiles, so a stale binary cannot be packed but the cold compile is not paid every image.
pub(crate) fn ensure_prerequisites(root: &Path, arch: Arch, profile: &Profile) -> Result<()> {
    let release = release_user_programs(
        profile.initfs.release,
        std::env::var("RAVEN_USER_RELEASE").ok().as_deref(),
    );
    if release {
        purge_release_userspace(root, arch)?;
    }
    let plan = prerequisite_plan(profile, arch, |source| root.join(source).is_file());
    let dynamic = profile.initfs.dynamic && build::dynamic_supported(arch);
    let needs_libc = brush_requested(profile, arch) || uutils_requested(profile, arch) || dynamic;
    let manifest_stale = dynamic && !crate::libc_link::loader_manifest_matches(root, arch.as_str());
    let sources_stale = needs_libc && !libc_stamp_current(root, arch);
    let dso_stale = dynamic && !crate::libc_link::dso_stamp_current(root, arch.as_str());
    if !plan.libc.is_empty() || manifest_stale || sources_stale || dso_stale {
        ensure_libc(root, arch, dynamic)?;
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

/// Run the libc pack when the arch archive or a crt object is missing, or relink the dynamic
/// loader when its manifest is missing or does not describe the file on disk.
pub(crate) fn ensure_libc(root: &Path, arch: Arch, dynamic: bool) -> Result<()> {
    let manifest_stale = dynamic && !crate::libc_link::loader_manifest_matches(root, arch.as_str());
    let sources_stale = !libc_stamp_current(root, arch);
    let dso_stale = dynamic && !crate::libc_link::dso_stamp_current(root, arch.as_str());
    let missing = libc_missing(arch, dynamic, |source| root.join(source).is_file());
    if missing.is_empty() && !manifest_stale && !sources_stale && !dso_stale {
        return Ok(());
    }
    if missing.is_empty() {
        if sources_stale {
            println!(
                "==> libc {}: packing changed sources (the sysroot stamp is stale)",
                arch.as_str()
            );
            let code =
                crate::libc_pack::run(root, &["--arch".to_string(), arch.as_str().to_string()])?;
            if code != 0 {
                bail!("libc-pack failed with exit code {code}");
            }
        } else {
            println!(
                "==> libc {}: relinking the dynamic loader (missing or stale manifest)",
                arch.as_str()
            );
        }
    } else {
        println!(
            "==> libc {}: packing missing prerequisites ({})",
            arch.as_str(),
            missing.join(", ")
        );
        let code = crate::libc_pack::run(root, &["--arch".to_string(), arch.as_str().to_string()])?;
        if code != 0 {
            bail!("libc-pack failed with exit code {code}");
        }
    }
    let dynamic_missing = missing
        .iter()
        .any(|source| DYNAMIC_OUTPUTS.iter().any(|name| source.ends_with(name)));
    if dynamic && (dynamic_missing || manifest_stale || sources_stale || dso_stale) {
        let code = crate::libc_link::run(root, &["--arch".to_string(), arch.as_str().to_string()])?;
        if code != 0 {
            bail!("libc-link failed with exit code {code}");
        }
    }
    let missing = libc_missing(arch, dynamic, |source| root.join(source).is_file());
    if !missing.is_empty() {
        bail!("libc-pack did not produce: {}", missing.join(", "));
    }
    if dynamic {
        verify_loader(root, arch)?;
    }
    Ok(())
}

/// Assert the loader on disk satisfies the K7 contract and its manifest.
fn verify_loader(root: &Path, arch: Arch) -> Result<()> {
    let loader = root.join("build").join(arch.as_str()).join("ld-elf.so.1");
    if let Err(error) = crate::libc_link::check_elf(&loader, "loader") {
        bail!("loader contract: {error}");
    }
    if !crate::libc_link::loader_manifest_matches(root, arch.as_str()) {
        bail!(
            "loader manifest is missing or does not match {}",
            loader.display()
        );
    }
    Ok(())
}

/// Assert the packed initfs carries the freshly built loader and its manifest contract.
///
/// The image builder calls this before writing the image, so a stale `build/<arch>/ld-elf.so.1`
/// can neither be silently packed nor leave an image behind. `dynamic` profiles without a
/// supported dynamic target are skipped.
pub(crate) fn verify_dynamic_payload(
    image: &[u8],
    root: &Path,
    arch: Arch,
    dynamic: bool,
) -> Result<()> {
    if !dynamic || !build::dynamic_supported(arch) {
        return Ok(());
    }
    let initfs = raven_initfs::Initfs::parse(image)
        .map_err(|error| anyhow!("cannot parse the packed initfs: {error:?}"))?;
    let loader = root.join("build").join(arch.as_str()).join("ld-elf.so.1");
    let loader_bytes = fs::read(&loader)
        .with_context(|| format!("cannot read the dynamic loader {}", loader.display()))?;
    let entry = initfs
        .get("/libexec/ld-elf.so.1")
        .ok_or_else(|| anyhow!("initfs does not contain /libexec/ld-elf.so.1"))?;
    if entry.data != loader_bytes.as_slice() {
        bail!(
            "the packed loader does not match the freshly built {}",
            loader.display()
        );
    }
    let manifest = crate::libc_link::read_loader_manifest(root, arch.as_str())
        .ok_or_else(|| anyhow!("loader manifest missing for {}", loader.display()))?;
    let expected_base = format!("0x{:x}", crate::libc_link::LOADER_IMAGE_BASE);
    if manifest.image_base != expected_base {
        bail!(
            "loader manifest image_base is {}, expected {expected_base}",
            manifest.image_base
        );
    }
    if manifest.interpreter != crate::libc_link::LOADER_INTERPRETER {
        bail!(
            "loader manifest interpreter is {}, expected {}",
            manifest.interpreter,
            crate::libc_link::LOADER_INTERPRETER
        );
    }
    if manifest.crc32 != format!("{:08x}", raven_initfs::crc32(&loader_bytes)) {
        bail!("loader manifest crc32 does not match {}", loader.display());
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

/// The ELF machine the target expects for a staged executable.
fn expected_machine(arch: Arch) -> Machine {
    match arch {
        Arch::X86_64 => Machine::X86_64,
        Arch::Aarch64 => Machine::Aarch64,
        Arch::Riscv64 => Machine::Riscv64,
    }
}

/// Mirror of `raven-abi`'s user mapping end; only the ELF header is inspected here.
const USER_END: u64 = 0x0000_7FFF_FFFF_F000;

/// Reject a staged executable whose ELF machine does not match the image architecture.
///
/// The target loader refuses a foreign-arch ELF at spawn (`bad ELF: BadMachine`). Staging must
/// fail at build time instead of embedding a binary the image cannot run. Non-ELF files
/// (configuration, firmware, device trees) are passed through unchanged.
pub(crate) fn verify_staged_elf(arch: Arch, source: &Path, data: &[u8]) -> Result<()> {
    if data.len() < 4 || !data.starts_with(b"\x7fELF") {
        return Ok(());
    }
    match Elf::parse(data, expected_machine(arch), 0..USER_END) {
        Err(raven_elf::ElfError::BadMachine) => bail!(
            "staged ELF {} is not a {} executable: the target loader rejects a foreign e_machine (bad ELF: BadMachine)",
            source.display(),
            arch.as_str()
        ),
        _ => Ok(()),
    }
}

/// Resolve a profile source; `{arch}` expands to the target and the legacy mutable
/// `build/brush` maps to the arch-correct path.
pub(crate) fn source_path(root: &Path, arch: Arch, source: &str) -> PathBuf {
    if source == "build/brush" {
        root.join(brush_path(arch))
    } else {
        root.join(source.replace("{arch}", arch.as_str()))
    }
}

/// Build the uutils slice once when the resolved profile stages a missing uutils binary.
///
/// The profile's `generated = true` entries are not validated by `Profile::load`, so
/// `make image`/`cargo xtask image` can reach the file copy with a clean `build/<arch>/uutils`.
/// The slice build is the same one `cargo xtask build uutils` runs; it is skipped when every
/// referenced staging file already exists so normal image builds stay fast.
pub(crate) fn ensure_uutils(root: &Path, arch: Arch, profile: &Profile) -> Result<()> {
    let exists = |source: &str| source_path(root, arch, source).is_file();
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
    let exists = |source: &str| source_path(root, arch, source).is_file();
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
    let concrete = format!("build/{}/uutils/", arch.as_str());
    let name = source
        .strip_prefix(&concrete)
        .or_else(|| source.strip_prefix("build/{arch}/uutils/"))?;
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
