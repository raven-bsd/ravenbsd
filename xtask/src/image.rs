// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Raw GPT image assembly: loader, kernel and initfs into a FAT32 ESP.

use std::fs::{self, File, OpenOptions};
use std::io::{Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail, ensure};

use crate::build::{self, Arch, Component};
use crate::fat::FatImageBuilder;
use crate::gpt::{GptLayout, Partition, SECTOR_SIZE, TYPE_ESP, TYPE_LINUX_DATA};
use crate::profiles::Profile;
use crate::repo::Workspace;

/// Image platform layout.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Platform {
    /// UEFI image: loader, kernel ELF, initfs and boot.toml in the ESP.
    Generic,
    /// Raspberry Pi 4: firmware, config.txt, flat kernel8.img and DTB in the FAT boot partition.
    Pi4,
}

impl Platform {
    /// Parse the `--platform` value.
    pub(crate) fn parse(value: &str) -> Result<Self> {
        match value {
            "generic" => Ok(Self::Generic),
            "pi4" => Ok(Self::Pi4),
            other => bail!("unknown platform `{other}` (expected generic or pi4)"),
        }
    }
}

pub(crate) const IMAGE_BYTES: u64 = 256 * 1024 * 1024;
pub(crate) const TOTAL_SECTORS: u64 = IMAGE_BYTES / SECTOR_SIZE;
pub(crate) const ESP_START_LBA: u64 = 2048;
pub(crate) const ESP_SECTORS: u64 = 64 * 1024 * 1024 / SECTOR_SIZE;
pub(crate) const ROOT_START_LBA: u64 = ESP_START_LBA + ESP_SECTORS;
pub(crate) const ROOT_SECTORS: u64 = 64 * 1024 * 1024 / SECTOR_SIZE;
pub(crate) const GPT_ENTRY_SECTORS: u64 = 32;

/// Build a bootable image from the profile and return 0 on success.
/// Optional image inputs supplied by the CLI or a fixture case.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct Overrides<'a> {
    /// Prepared initfs instead of the profile build.
    pub(crate) initfs: Option<&'a Path>,
    /// Prepared kernel instead of the fresh kernel build.
    pub(crate) kernel: Option<&'a Path>,
    /// Output path instead of the platform default.
    pub(crate) output: Option<&'a Path>,
}

pub(crate) fn run(
    root: &Path,
    arch: Arch,
    profile_name: &str,
    cmdline: Option<&str>,
    output: Option<&Path>,
) -> Result<u8> {
    run_with_overrides(
        root,
        arch,
        profile_name,
        cmdline,
        Overrides {
            output,
            ..Overrides::default()
        },
        Platform::Generic,
    )
}

/// Build a bootable image and return 0 on success.
///
/// The `overrides` fields install a prepared file instead of the profile's initfs and the freshly
/// built kernel (fixture cases and the loader test kernel). Relative paths are resolved against
/// the repository root.
pub(crate) fn run_with_overrides(
    root: &Path,
    arch: Arch,
    profile_name: &str,
    cmdline: Option<&str>,
    overrides: Overrides<'_>,
    platform: Platform,
) -> Result<u8> {
    if platform == Platform::Pi4 {
        return crate::pi::build(
            root,
            arch,
            profile_name,
            cmdline,
            overrides.initfs,
            overrides.kernel,
            overrides.output,
        );
    }
    let profile = Profile::load(root, profile_name)?;
    let workspace = Workspace::load(root)?;

    // The loader is always needed. A test kernel override replaces the real kernel, so there is
    // no reason to build it.
    let code = build::run(&workspace, Component::Loader, arch, false, &[])?;
    if code != 0 {
        bail!("loader build failed with exit code {code}");
    }
    if overrides.kernel.is_none() {
        let code = build::run(&workspace, Component::Kernel, arch, false, &[])?;
        if code != 0 {
            bail!("kernel build failed with exit code {code}");
        }
    }

    // A rootfs profile is a production image: its initfs and root programs are built with the
    // release profile so the initfs stays small enough for the RAM block image.
    let rootfs = crate::rootfs::profile_exists(root, profile_name);
    let initfs_bytes = match overrides.initfs {
        Some(path) => {
            let path = resolve_override(root, path);
            fs::read(&path).with_context(|| format!("cannot read {}", path.display()))?
        }
        None if rootfs => build_release_initfs(root, arch, &profile, &workspace)?,
        None => {
            let path = crate::initfs::build(root, arch, profile_name)?;
            fs::read(&path).with_context(|| format!("cannot read {}", path.display()))?
        }
    };
    let kernel_path = match overrides.kernel {
        Some(path) => resolve_override(root, path),
        None => build::artifact_path(root, arch, Component::Kernel, ""),
    };

    let loader = build::artifact_path(root, arch, Component::Loader, "");
    let loader_bytes =
        fs::read(&loader).with_context(|| format!("cannot read {}", loader.display()))?;
    let kernel_bytes =
        fs::read(&kernel_path).with_context(|| format!("cannot read {}", kernel_path.display()))?;
    // The `hw` profile carries a UFS root. Until virtio-blk lands, blockd serves it from the
    // initfs as a RAM disk, so the one-partition GPT disk is inserted at /etc/blockd.img.
    let initfs_bytes = match crate::rootfs::build_for_profile(root, arch, profile_name, true)? {
        Some(disk) => {
            let data =
                fs::read(&disk).with_context(|| format!("cannot read {}", disk.display()))?;
            crate::rootfs::inject_initfs_file(&initfs_bytes, "/etc/blockd.img", &data, 0o444)?
        }
        None => initfs_bytes,
    };

    let boot_toml = boot_toml(&profile, cmdline);
    let loader_name = match arch {
        Arch::X86_64 => "EFI/BOOT/BOOTX64.EFI",
        Arch::Aarch64 => "EFI/BOOT/BOOTAA64.EFI",
        Arch::Riscv64 => bail!("loader is not supported for riscv64"),
    };

    let mut fat = FatImageBuilder::new(usize::try_from(ESP_SECTORS * SECTOR_SIZE)?)?
        .with_epoch(source_date_epoch());
    for dir in ["EFI", "EFI/BOOT", "raven"] {
        fat.add_dir(dir)?;
    }
    fat.add_file(loader_name, &loader_bytes)?;
    fat.add_file("raven/kernel.elf", &kernel_bytes)?;
    fat.add_file("raven/initfs.img", &initfs_bytes)?;
    fat.add_file("raven/boot.toml", boot_toml.as_bytes())?;
    let esp = fat.finish()?;
    ensure!(
        esp.len() as u64 == ESP_SECTORS * SECTOR_SIZE,
        "ESP image has the wrong size"
    );

    let gpt = GptLayout::new(
        TOTAL_SECTORS,
        vec![
            Partition::new(
                TYPE_ESP,
                0,
                ESP_START_LBA,
                ESP_START_LBA + ESP_SECTORS - 1,
                "ESP",
            ),
            Partition::new(
                TYPE_LINUX_DATA,
                1,
                ROOT_START_LBA,
                ROOT_START_LBA + ROOT_SECTORS - 1,
                "ROOT",
            ),
        ],
    )?;

    let output = overrides
        .output
        .map(Path::to_path_buf)
        .unwrap_or_else(|| default_output(root, arch));
    if let Some(parent) = output.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("cannot create {}", parent.display()))?;
    }
    write_image(&output, &gpt, &esp)?;
    println!("{}", output.display());
    Ok(0)
}

/// Build the initfs for a rootfs profile from release user programs.
fn build_release_initfs(
    root: &Path,
    arch: Arch,
    profile: &Profile,
    workspace: &Workspace,
) -> Result<Vec<u8>> {
    let mut packages: Vec<String> = profile
        .initfs
        .programs
        .iter()
        .map(|program| program.package.clone())
        .collect();
    packages.sort();
    packages.dedup();
    if !packages.is_empty() {
        let code = build::run(workspace, Component::User, arch, true, &packages)?;
        if code != 0 {
            bail!("release user program build failed with exit code {code}");
        }
    }
    let mut builder = raven_initfs::InitfsBuilder::new();
    for program in &profile.initfs.programs {
        let path = build::artifact_path(root, arch, Component::User, &program.package);
        let data = fs::read(&path).with_context(|| {
            format!(
                "cannot read release user program {} (package `{}`)",
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
    Ok(builder.build())
}

fn default_output(root: &Path, arch: Arch) -> PathBuf {
    root.join("build")
        .join(arch.as_str())
        .join(format!("ravenbsd-{}.img", arch.as_str()))
}

/// Absolute path of an image override; relative paths are anchored at the repository root.
fn resolve_override(root: &Path, path: &Path) -> PathBuf {
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        root.join(path)
    }
}

pub(crate) fn write_image(path: &Path, gpt: &GptLayout, esp: &[u8]) -> Result<()> {
    let mut file = OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .open(path)
        .with_context(|| format!("cannot create {}", path.display()))?;
    file.set_len(IMAGE_BYTES)
        .with_context(|| format!("cannot size {}", path.display()))?;

    write_at(&mut file, 0, &gpt.protective_mbr())?;
    write_at(&mut file, SECTOR_SIZE, &gpt.primary_header())?;
    write_at(&mut file, SECTOR_SIZE * 2, &gpt.entry_array())?;
    let backup_entries_lba = gpt.total_sectors() - 1 - GPT_ENTRY_SECTORS;
    write_at(
        &mut file,
        backup_entries_lba * SECTOR_SIZE,
        &gpt.entry_array(),
    )?;
    write_at(
        &mut file,
        (gpt.total_sectors() - 1) * SECTOR_SIZE,
        &gpt.backup_header(),
    )?;
    write_at(&mut file, ESP_START_LBA * SECTOR_SIZE, esp)?;
    file.flush()
        .with_context(|| format!("cannot flush {}", path.display()))?;
    Ok(())
}

fn write_at(file: &mut File, offset: u64, bytes: &[u8]) -> Result<()> {
    file.seek(SeekFrom::Start(offset))?;
    file.write_all(bytes)?;
    Ok(())
}

/// Render the profile's boot table, with `--cmdline` overriding the profile.
pub(crate) fn boot_toml(profile: &Profile, cmdline: Option<&str>) -> String {
    let cmdline = cmdline.unwrap_or(&profile.boot.cmdline);
    format!(
        "cmdline = \"{}\"\nlog_level = \"{}\"\n",
        toml_escape(cmdline),
        toml_escape(&profile.boot.log_level)
    )
}

fn toml_escape(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len());
    for character in value.chars() {
        match character {
            '\\' => escaped.push_str("\\\\"),
            '"' => escaped.push_str("\\\""),
            '\n' => escaped.push_str("\\n"),
            '\r' => escaped.push_str("\\r"),
            '\t' => escaped.push_str("\\t"),
            other => escaped.push(other),
        }
    }
    escaped
}

pub(crate) fn source_date_epoch() -> Option<i64> {
    std::env::var("SOURCE_DATE_EPOCH")
        .ok()
        .and_then(|value| value.parse().ok())
}

#[cfg(test)]
#[path = "image_ut.rs"]
mod tests;
