// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Build the UFS root filesystem used by the `hw` image.
//!
//! A rootfs profile is a TOML file `rootfs/<profile>.toml`. Its `[[program]]` entries name
//! workspace packages and their `/sbin` or `/bin` install path; `[[file]]` entries copy repository
//! files. `make image PROFILE=hw` stages that tree, formats a UFS2 image and wraps it in the
//! one-partition GPT disk `blockd` expects. Until the virtio-blk driver lands, the disk is embedded
//! in the initfs as `/etc/blockd.img` and served by the RAM block device.

use std::collections::BTreeSet;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Component as PathComponent, Path, PathBuf};

use anyhow::{Context, Result, bail, ensure};
use serde::Deserialize;
use ufs_mkfs::{MkfsOptions, format_image};

use crate::build::{self, Arch, Component};
use crate::gpt::{GptLayout, Partition, SECTOR_SIZE};
use crate::profiles::Profile;
use crate::repo::Workspace;

/// FreeBSD UFS type GUID `516E7CB6-6ECF-11D6-8FF8-00022D09712B`.
pub(crate) const TYPE_FREEBSD_UFS: [u8; 16] = [
    0xb6, 0x7c, 0x6e, 0x51, 0xcf, 0x6e, 0xd6, 0x11, 0x8f, 0xf8, 0x00, 0x02, 0x2d, 0x09, 0x71, 0x2b,
];
const FIRST_USABLE_LBA: u64 = 34;
const BACKUP_SECTORS: u64 = 33;

/// One rootfs program: a workspace package and its absolute target path.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct RootProgram {
    package: String,
    path: String,
}

/// One repository file copied into the rootfs staging tree.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct RootFile {
    src: String,
    path: String,
    #[serde(default = "default_mode")]
    mode: u32,
    /// The source is a generated build artifact under `build/`.
    #[serde(default)]
    generated: bool,
}

const fn default_mode() -> u32 {
    0o644
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct RootSpec {
    size: u64,
    #[serde(default)]
    programs: Vec<RootProgram>,
    #[serde(default)]
    files: Vec<RootFile>,
}

/// Create a deterministic UFS2 root image from a staging directory.
pub fn build_ufs_root(staging_dir: &Path, size: u64, out: &Path) -> Result<()> {
    let timestamp = std::env::var("SOURCE_DATE_EPOCH")
        .ok()
        .and_then(|value| value.parse::<i64>().ok())
        .unwrap_or(0);
    let options = MkfsOptions {
        size,
        version: 2,
        block_size: 4096,
        fragment_size: 512,
        bytes_per_inode: 4096,
        minfree: 5,
        label: "ravenbsd-root",
        timestamp,
    };
    format_image(out, &options, Some(staging_dir))
        .map_err(|error| anyhow::anyhow!("build UFS root {}: {error}", out.display()))
}

/// Wrap a raw UFS image in a deterministic one-partition GPT disk.
///
/// The layout matches the K54 RAM-block fixture: payload at LBA 34, a backup entry array and
/// header at the end, and the FreeBSD UFS partition type GUID.
pub fn build_root_disk(ufs_image: &Path, out: &Path) -> Result<()> {
    let payload =
        fs::read(ufs_image).with_context(|| format!("cannot read {}", ufs_image.display()))?;
    ensure!(
        !payload.is_empty() && payload.len().is_multiple_of(SECTOR_SIZE as usize),
        "{} is not a whole number of 512-byte sectors",
        ufs_image.display()
    );
    let payload_sectors = payload.len() as u64 / SECTOR_SIZE;
    let total = FIRST_USABLE_LBA + payload_sectors + BACKUP_SECTORS;
    let layout = GptLayout::new(
        total,
        vec![Partition::new(
            TYPE_FREEBSD_UFS,
            0,
            FIRST_USABLE_LBA,
            FIRST_USABLE_LBA + payload_sectors - 1,
            "ufs-root",
        )],
    )?;
    let bytes = total
        .checked_mul(SECTOR_SIZE)
        .and_then(|value| usize::try_from(value).ok())
        .ok_or_else(|| anyhow::anyhow!("root disk is too large"))?;
    let mut disk = vec![0u8; bytes];
    write_at(&mut disk, 0, &layout.protective_mbr())?;
    write_at(&mut disk, SECTOR_SIZE, &layout.primary_header())?;
    write_at(&mut disk, 2 * SECTOR_SIZE, &layout.entry_array())?;
    let backup_entries = total - 1 - 32;
    write_at(
        &mut disk,
        backup_entries * SECTOR_SIZE,
        &layout.entry_array(),
    )?;
    write_at(
        &mut disk,
        (total - 1) * SECTOR_SIZE,
        &layout.backup_header(),
    )?;
    write_at(&mut disk, FIRST_USABLE_LBA * SECTOR_SIZE, &payload)?;
    if let Some(parent) = out.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("cannot create {}", parent.display()))?;
    }
    fs::write(out, disk).with_context(|| format!("cannot write {}", out.display()))?;
    Ok(())
}

/// Insert one file into an existing initfs image.
pub fn inject_initfs_file(image: &[u8], path: &str, data: &[u8], mode: u32) -> Result<Vec<u8>> {
    let parsed = raven_initfs::Initfs::parse(image)
        .map_err(|error| anyhow::anyhow!("cannot parse initfs: {error:?}"))?;
    ensure!(parsed.get(path).is_none(), "initfs already contains {path}");
    let mut builder = raven_initfs::InitfsBuilder::new();
    for entry in parsed.entries() {
        builder.add_file(entry.path, entry.data, entry.mode)?;
    }
    builder.add_file(path, data, mode)?;
    Ok(builder.build())
}

/// Stage a rootfs spec without building workspace packages.
///
/// This is the files-only part used by `build_for_profile` after the programs have been built and
/// is public so the host test can prove that removing a manifest removes it from the staged tree.
pub fn stage_rootfs_spec(source_root: &Path, spec_path: &Path, staging: &Path) -> Result<()> {
    let spec = load_spec(spec_path)?;
    if !spec.programs.is_empty() {
        bail!(
            "{}: program entries need the full rootfs build path",
            spec_path.display()
        );
    }
    reset_directory(staging)?;
    stage_files(source_root, &spec.files, staging)
}

/// Resolve a rootfs spec: `rootfs/<profile>.toml`, or the profile-adjacent
/// `profiles/<profile>/rootfs.toml` used by the Pi 4 SD image.
pub(crate) fn spec_path(root: &Path, profile: &str) -> Option<PathBuf> {
    let conventional = root.join("rootfs").join(format!("{profile}.toml"));
    if conventional.is_file() {
        return Some(conventional);
    }
    let profile_adjacent = root.join("profiles").join(profile).join("rootfs.toml");
    profile_adjacent.is_file().then_some(profile_adjacent)
}

/// True when a rootfs spec exists for `profile`.
pub fn profile_exists(root: &Path, profile: &str) -> bool {
    spec_path(root, profile).is_some()
}

/// Build the root image for `profile` when `rootfs/<profile>.toml` exists.
pub fn build_for_profile(
    root: &Path,
    arch: Arch,
    profile: &str,
    release: bool,
) -> Result<Option<PathBuf>> {
    let Some(spec_path) = spec_path(root, profile) else {
        return Ok(None);
    };
    let spec = load_spec(&spec_path)?;
    ensure!(
        spec.size >= 1024 * 1024 && spec.size.is_multiple_of(4096),
        "{}: rootfs size must be a 4 KiB multiple of at least 1 MiB",
        spec_path.display()
    );
    let mut seen = BTreeSet::new();
    for program in &spec.programs {
        if !seen.insert(program.path.clone()) {
            bail!(
                "{}: duplicate rootfs path `{}`",
                spec_path.display(),
                program.path
            );
        }
    }
    for file in &spec.files {
        if !seen.insert(file.path.clone()) {
            bail!(
                "{}: duplicate rootfs path `{}`",
                spec_path.display(),
                file.path
            );
        }
    }
    let workspace = Workspace::load(root)?;
    let packages: Vec<String> = spec
        .programs
        .iter()
        .map(|program| program.package.clone())
        .collect();
    if !packages.is_empty() {
        let code = build::run(&workspace, Component::User, arch, release, &packages, false)?;
        if code != 0 {
            bail!("rootfs user program build failed with exit code {code}");
        }
    }

    let staging = root
        .join("build")
        .join(arch.as_str())
        .join("rootfs-staging");
    reset_directory(&staging)?;
    for program in &spec.programs {
        let source = build::artifact_path(root, arch, Component::User, &program.package);
        let destination = staging_path(spec_path.as_path(), &program.path, &staging)?;
        if let Some(parent) = destination.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("cannot create {}", parent.display()))?;
        }
        fs::copy(&source, &destination).with_context(|| {
            format!(
                "cannot copy rootfs program {} -> {}",
                source.display(),
                destination.display()
            )
        })?;
        set_mode(&destination, 0o755)?;
    }
    stage_files(root, &spec.files, &staging)?;

    let ufs = root.join("build").join(arch.as_str()).join("rootfs.ufs");
    build_ufs_root(&staging, spec.size, &ufs)?;
    let disk = root.join("build").join(arch.as_str()).join("blockd.img");
    build_root_disk(&ufs, &disk)?;
    Ok(Some(disk))
}

/// Build the raw UFS image for the Pi 4 SD root from the resolved profile's initfs
/// userland plus the profile-adjacent rootfs spec.
///
/// The profile is the single source of truth for the programs, services and `/etc` files;
/// the spec only fixes the UFS size and adds root-only files such as `/root/.keep`.
pub(crate) fn build_sd_root(
    root: &Path,
    arch: Arch,
    profile_name: &str,
    release: bool,
) -> Result<PathBuf> {
    let Some(spec_path) = spec_path(root, profile_name) else {
        bail!("no rootfs spec for profile `{profile_name}`");
    };
    let spec = load_spec(&spec_path)?;
    ensure!(
        spec.size >= 1024 * 1024 && spec.size.is_multiple_of(4096),
        "{}: rootfs size must be a 4 KiB multiple of at least 1 MiB",
        spec_path.display()
    );
    let profile = Profile::load(root, profile_name)?;
    let workspace = Workspace::load(root)?;

    let packages: Vec<String> = profile
        .initfs
        .programs
        .iter()
        .map(|program| program.package.clone())
        .collect();
    if !packages.is_empty() {
        let code = build::run(&workspace, Component::User, arch, release, &packages, false)?;
        if code != 0 {
            bail!("rootfs user program build failed with exit code {code}");
        }
    }

    let mut seen = BTreeSet::new();
    for program in &profile.initfs.programs {
        if !seen.insert(program.path.clone()) {
            bail!(
                "{}: duplicate rootfs path `{}`",
                spec_path.display(),
                program.path
            );
        }
    }
    for file in &profile.initfs.files {
        if !seen.insert(file.path.clone()) {
            bail!(
                "{}: duplicate rootfs path `{}`",
                spec_path.display(),
                file.path
            );
        }
    }
    let mut spec_seen = BTreeSet::new();
    for file in &spec.files {
        if !spec_seen.insert(file.path.clone()) {
            bail!(
                "{}: duplicate rootfs path `{}`",
                spec_path.display(),
                file.path
            );
        }
        // A spec file may overlay a profile file (for example the post-pivot
        // /etc/mounts); it is staged after the profile below.
    }

    let staging = root
        .join("build")
        .join(arch.as_str())
        .join("rootfs-staging");
    reset_directory(&staging)?;
    for program in &profile.initfs.programs {
        let source = build::artifact_path(root, arch, Component::User, &program.package);
        let destination = staging_path(&spec_path, &program.path, &staging)?;
        if let Some(parent) = destination.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("cannot create {}", parent.display()))?;
        }
        fs::copy(&source, &destination).with_context(|| {
            format!(
                "cannot copy rootfs program {} -> {}",
                source.display(),
                destination.display()
            )
        })?;
        set_mode(&destination, 0o755)?;
    }
    for file in &profile.initfs.files {
        stage_root_file(
            root,
            arch,
            &staging,
            &spec_path,
            &file.src,
            &file.path,
            file.mode,
            file.generated,
        )?;
    }
    for file in &spec.files {
        stage_root_file(
            root,
            arch,
            &staging,
            &spec_path,
            &file.src,
            &file.path,
            file.mode,
            file.generated,
        )?;
    }

    let ufs = root.join("build").join(arch.as_str()).join("rootfs.ufs");
    build_ufs_root(&staging, spec.size, &ufs)?;
    Ok(ufs)
}

#[allow(clippy::too_many_arguments)]
fn stage_root_file(
    source_root: &Path,
    arch: Arch,
    staging: &Path,
    spec_path: &Path,
    src: &str,
    path: &str,
    mode: u32,
    generated: bool,
) -> Result<()> {
    let source = crate::initfs::source_path(source_root, arch, src);
    if !source.is_file() && !generated {
        bail!("rootfs source `{src}` does not exist");
    }
    let destination = staging_path(spec_path, path, staging)?;
    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("cannot create {}", parent.display()))?;
    }
    fs::copy(&source, &destination).with_context(|| {
        format!(
            "cannot copy rootfs file {} -> {}",
            src,
            destination.display()
        )
    })?;
    set_mode(&destination, mode)
}

fn load_spec(path: &Path) -> Result<RootSpec> {
    let text =
        fs::read_to_string(path).with_context(|| format!("cannot read {}", path.display()))?;
    toml::from_str(&text).with_context(|| format!("cannot parse {}", path.display()))
}

fn reset_directory(path: &Path) -> Result<()> {
    if path.exists() {
        fs::remove_dir_all(path).with_context(|| format!("cannot remove {}", path.display()))?;
    }
    fs::create_dir_all(path).with_context(|| format!("cannot create {}", path.display()))
}

fn stage_files(source_root: &Path, files: &[RootFile], staging: &Path) -> Result<()> {
    let mut destinations = BTreeSet::new();
    for file in files {
        let source = Path::new(&file.src);
        if source.is_absolute()
            || source
                .components()
                .any(|part| matches!(part, PathComponent::ParentDir))
            || file.src.is_empty()
        {
            bail!("rootfs source `{}` is not repo-relative", file.src);
        }
        if !source_root.join(source).is_file() && !file.generated {
            bail!("rootfs source `{}` does not exist", file.src);
        }
        let destination = staging_path(Path::new("rootfs"), &file.path, staging)?;
        if !destinations.insert(file.path.clone()) {
            bail!("duplicate rootfs path `{}`", file.path);
        }
        if let Some(parent) = destination.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("cannot create {}", parent.display()))?;
        }
        fs::copy(source_root.join(source), &destination).with_context(|| {
            format!(
                "cannot copy rootfs file {} -> {}",
                file.src,
                destination.display()
            )
        })?;
        set_mode(&destination, file.mode)?;
    }
    Ok(())
}

fn staging_path(spec_path: &Path, path: &str, staging: &Path) -> Result<PathBuf> {
    if !path.starts_with('/') || path.contains('\0') {
        bail!(
            "{}: rootfs path `{path}` is not absolute",
            spec_path.display()
        );
    }
    let relative = path.trim_start_matches('/');
    if relative.is_empty()
        || Path::new(relative)
            .components()
            .any(|part| !matches!(part, PathComponent::Normal(_)))
    {
        bail!("{}: invalid rootfs path `{path}`", spec_path.display());
    }
    Ok(staging.join(relative))
}

fn set_mode(path: &Path, mode: u32) -> Result<()> {
    fs::set_permissions(path, fs::Permissions::from_mode(mode))
        .with_context(|| format!("cannot set mode on {}", path.display()))
}

fn write_at(disk: &mut [u8], offset: u64, bytes: &[u8]) -> Result<()> {
    let start = usize::try_from(offset).map_err(|_| anyhow::anyhow!("disk offset overflow"))?;
    let end = start
        .checked_add(bytes.len())
        .ok_or_else(|| anyhow::anyhow!("disk offset overflow"))?;
    disk.get_mut(start..end)
        .ok_or_else(|| anyhow::anyhow!("disk write outside the image"))?
        .copy_from_slice(bytes);
    Ok(())
}

#[cfg(test)]
#[path = "rootfs_ut.rs"]
mod tests;
