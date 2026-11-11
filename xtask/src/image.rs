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

const IMAGE_BYTES: u64 = 256 * 1024 * 1024;
const TOTAL_SECTORS: u64 = IMAGE_BYTES / SECTOR_SIZE;
const ESP_START_LBA: u64 = 2048;
const ESP_SECTORS: u64 = 64 * 1024 * 1024 / SECTOR_SIZE;
const ROOT_START_LBA: u64 = ESP_START_LBA + ESP_SECTORS;
const ROOT_SECTORS: u64 = 64 * 1024 * 1024 / SECTOR_SIZE;
const GPT_ENTRY_SECTORS: u64 = 32;

/// Build a bootable image from the profile and return 0 on success.
pub(crate) fn run(
    root: &Path,
    arch: Arch,
    profile_name: &str,
    cmdline: Option<&str>,
    output: Option<&Path>,
) -> Result<u8> {
    run_with_overrides(root, arch, profile_name, cmdline, None, None, output)
}

/// Build a bootable image and return 0 on success.
///
/// `initfs_override` and `kernel_override` install a prepared file instead of the profile's
/// initfs and the freshly built kernel (fixture cases and the loader test kernel). Relative
/// paths are resolved against the repository root.
pub(crate) fn run_with_overrides(
    root: &Path,
    arch: Arch,
    profile_name: &str,
    cmdline: Option<&str>,
    initfs_override: Option<&Path>,
    kernel_override: Option<&Path>,
    output: Option<&Path>,
) -> Result<u8> {
    let profile = Profile::load(root, profile_name)?;
    let workspace = Workspace::load(root)?;

    // The loader is always needed. A test kernel override replaces the real kernel, so there is
    // no reason to build it.
    let code = build::run(&workspace, Component::Loader, arch, false, &[])?;
    if code != 0 {
        bail!("loader build failed with exit code {code}");
    }
    if kernel_override.is_none() {
        let code = build::run(&workspace, Component::Kernel, arch, false, &[])?;
        if code != 0 {
            bail!("kernel build failed with exit code {code}");
        }
    }

    let initfs_path = match initfs_override {
        Some(path) => resolve_override(root, path),
        None => crate::initfs::build(root, arch, profile_name)?,
    };
    let kernel_path = match kernel_override {
        Some(path) => resolve_override(root, path),
        None => build::artifact_path(root, arch, Component::Kernel, ""),
    };

    let loader = build::artifact_path(root, arch, Component::Loader, "");
    let loader_bytes =
        fs::read(&loader).with_context(|| format!("cannot read {}", loader.display()))?;
    let kernel_bytes =
        fs::read(&kernel_path).with_context(|| format!("cannot read {}", kernel_path.display()))?;
    let initfs_bytes =
        fs::read(&initfs_path).with_context(|| format!("cannot read {}", initfs_path.display()))?;

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

    let output = output
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

fn write_image(path: &Path, gpt: &GptLayout, esp: &[u8]) -> Result<()> {
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

fn source_date_epoch() -> Option<i64> {
    std::env::var("SOURCE_DATE_EPOCH")
        .ok()
        .and_then(|value| value.parse().ok())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn boot_toml_uses_profile_and_override() {
        let profile = Profile::load(
            Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap(),
            "base",
        )
        .unwrap();
        assert_eq!(
            boot_toml(&profile, None),
            "cmdline = \"console=ttyS0 test-exit=1\"\nlog_level = \"info\"\n"
        );
        assert_eq!(
            boot_toml(&profile, Some("console=ttyS0 \"quoted\"")),
            "cmdline = \"console=ttyS0 \\\"quoted\\\"\"\nlog_level = \"info\"\n"
        );
    }

    #[test]
    fn default_output_is_under_build_arch() {
        assert_eq!(
            default_output(Path::new("/repo"), Arch::X86_64),
            Path::new("/repo/build/x86_64/ravenbsd-x86_64.img")
        );
    }

    #[test]
    fn relative_overrides_are_anchored_at_the_root() {
        let root = Path::new("/repo");
        assert_eq!(
            resolve_override(root, Path::new("build/fixtures/s24-spawn/initfs.img")),
            Path::new("/repo/build/fixtures/s24-spawn/initfs.img")
        );
        assert_eq!(
            resolve_override(root, Path::new("/tmp/kernel.elf")),
            Path::new("/tmp/kernel.elf")
        );
    }
}
