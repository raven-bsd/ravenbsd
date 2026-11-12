// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Raspberry Pi 4 SD-image assembly.
//!
//! The Pi firmware loads a flat `kernel8.img` at `0x80000`, not an ELF. This module builds the
//! kernel with the S15a Pi linker script, converts the ELF load segments to the flat image, appends
//! the initfs, and writes the firmware files plus `config.txt` into the FAT boot partition. The
//! firmware and DTB are read from the read-only donor checkout at build time and are never
//! committed.

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail, ensure};

use crate::build::{self, Arch, Component};
use crate::fat::FatImageBuilder;
use crate::gpt::{GptLayout, Partition, SECTOR_SIZE, TYPE_ESP, TYPE_LINUX_DATA};
use crate::image::{
    ESP_SECTORS, ESP_START_LBA, IMAGE_BYTES, ROOT_SECTORS, ROOT_START_LBA, source_date_epoch,
    write_image,
};
use crate::process::{CommandSpec, run};
use crate::profiles::Profile;
use crate::repo::{self, Workspace};

/// The Pi firmware loads the flat image at this physical address and enters its first byte.
const PI_LOAD: u64 = 0x80000;
/// The DTB for the first Pi 4 board revision family.
const DTB_NAME: &str = "bcm2711-rpi-4-b.dtb";
/// Firmware files the boot partition needs, plus the DTB and the Broadcom licence.
const FIRMWARE_FILES: [&str; 5] = [
    "bootcode.bin",
    "start4.elf",
    "fixup4.dat",
    "LICENCE.broadcom",
    DTB_NAME,
];
/// The exact config.txt required by the K35 contract.
const CONFIG_TXT: &str = "arm_64bit=1\nkernel=kernel8.img\nenable_uart=1\nuart_2ndstage=1\n";

/// Build a Pi 4 image.
pub(crate) fn build(
    root: &Path,
    arch: Arch,
    profile_name: &str,
    _cmdline: Option<&str>,
    initfs_override: Option<&Path>,
    kernel_override: Option<&Path>,
    output: Option<&Path>,
) -> Result<u8> {
    ensure!(
        arch == Arch::Aarch64,
        "the pi4 platform requires --arch aarch64"
    );
    // The profile still builds the normal initfs; the Pi boot partition only changes the image
    // layout, not the root contents.
    let _profile = Profile::load(root, profile_name)?;
    let workspace = Workspace::load(root)?;
    let initfs_path = match initfs_override {
        Some(path) => resolve_override(root, path),
        None => crate::initfs::build(root, arch, profile_name)?,
    };
    let initfs =
        fs::read(&initfs_path).with_context(|| format!("cannot read {}", initfs_path.display()))?;
    let kernel_elf = match kernel_override {
        Some(path) => {
            let path = resolve_override(root, path);
            fs::read(&path).with_context(|| format!("cannot read {}", path.display()))?
        }
        None => build_kernel_elf(root, &workspace)?,
    };
    let kernel8 = kernel8_image(&kernel_elf, &initfs)?;
    let firmware = firmware_dir(root)?;
    let output = resolve_output(root, output);
    if let Some(parent) = output.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("cannot create {}", parent.display()))?;
    }
    let esp = build_esp(&kernel8, &firmware)?;
    let gpt = GptLayout::new(
        IMAGE_BYTES / SECTOR_SIZE,
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
    write_image(&output, &gpt, &esp)?;
    println!("{}", output.display());
    Ok(0)
}

/// Build the Pi kernel with `RAVEN_PI4=1` and return its ELF bytes.
fn build_kernel_elf(root: &Path, workspace: &Workspace) -> Result<Vec<u8>> {
    let package = workspace
        .kernel()
        .ok_or_else(|| anyhow::anyhow!("component 'kernel' does not exist yet"))?
        .name
        .clone();
    let target = build::target_for(Component::Kernel, Arch::Aarch64)?;
    let target_dir = root.join("build/cargo/kernel-aarch64-pi4");
    let flags = build::rustflags_string(Component::Kernel, Arch::Aarch64, root, None);
    let head = repo::head_info(root)?;
    let build_id = if head.dirty {
        format!("{}-dirty", head.short)
    } else {
        head.short.clone()
    };

    let args = vec![
        "build".to_string(),
        "-p".to_string(),
        package,
        "--target".to_string(),
        target.to_string(),
        "--target-dir".to_string(),
        target_dir.display().to_string(),
    ];
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
    println!("==> build kernel aarch64 (pi4, {target})");
    let code = run(&spec)?;
    if code != 0 {
        bail!("Pi kernel build failed with exit code {code}");
    }
    let artifact = target_dir.join(target).join("debug").join("raven-kernel");
    fs::read(&artifact).with_context(|| format!("cannot read {}", artifact.display()))
}

/// Flatten the ELF load segments at `0x80000` and append the initfs.
fn kernel8_image(elf: &[u8], initfs: &[u8]) -> Result<Vec<u8>> {
    let mut image = flatten_elf(elf)?;
    let total = image
        .len()
        .checked_add(initfs.len())
        .ok_or_else(|| anyhow::anyhow!("kernel8 image length overflow"))?;
    ensure!(
        total < usize::try_from(ESP_SECTORS * SECTOR_SIZE).unwrap_or(usize::MAX),
        "kernel8.img does not fit the FAT boot partition"
    );
    image.extend_from_slice(initfs);
    Ok(image)
}

/// Convert an aarch64 ELF to the flat image the Pi firmware loads at `0x80000`.
fn flatten_elf(elf: &[u8]) -> Result<Vec<u8>> {
    let segments = load_segments(elf)?;
    ensure!(!segments.is_empty(), "kernel ELF has no loadable segments");
    let mut end = PI_LOAD;
    for segment in &segments {
        if segment.paddr < PI_LOAD {
            bail!(
                "kernel segment at {:#x} is below the Pi load address {:#x}",
                segment.paddr,
                PI_LOAD
            );
        }
        let segment_end = segment
            .paddr
            .checked_add(segment.memsz)
            .ok_or_else(|| anyhow::anyhow!("kernel segment end overflow"))?;
        end = end.max(segment_end);
    }
    let length =
        usize::try_from(end - PI_LOAD).map_err(|_| anyhow::anyhow!("kernel image is too large"))?;
    let mut image = vec![0u8; length];
    let mut previous_end = 0u64;
    for segment in &segments {
        let start = usize::try_from(segment.paddr - PI_LOAD)
            .map_err(|_| anyhow::anyhow!("kernel segment offset is too large"))?;
        let size = usize::try_from(segment.filesz)
            .map_err(|_| anyhow::anyhow!("kernel segment is too large"))?;
        let finish = start
            .checked_add(size)
            .ok_or_else(|| anyhow::anyhow!("kernel segment end overflow"))?;
        let source_end = usize::try_from(segment.offset)
            .ok()
            .and_then(|offset| offset.checked_add(size))
            .ok_or_else(|| anyhow::anyhow!("kernel segment file range overflow"))?;
        ensure!(
            finish <= image.len() && source_end <= elf.len() && segment.filesz <= segment.memsz,
            "kernel segment is outside the ELF or flat image"
        );
        ensure!(
            segment.paddr >= previous_end,
            "kernel load segments overlap in the flat image"
        );
        image[start..finish].copy_from_slice(&elf[segment.offset as usize..source_end]);
        previous_end = segment.paddr + segment.memsz;
    }
    Ok(image)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct LoadSegment {
    offset: u64,
    paddr: u64,
    filesz: u64,
    memsz: u64,
}

fn load_segments(elf: &[u8]) -> Result<Vec<LoadSegment>> {
    ensure!(elf.len() >= 64, "kernel ELF is shorter than the header");
    ensure!(&elf[..4] == b"\x7fELF", "kernel is not an ELF file");
    ensure!(elf[4] == 2, "kernel ELF is not 64-bit");
    ensure!(elf[5] == 1, "kernel ELF is not little-endian");
    let machine = read_u16(elf, 18)?;
    ensure!(machine == 183, "kernel ELF is not aarch64");
    let phoff = read_u64(elf, 32)?;
    let phentsize = read_u16(elf, 54)?;
    let phnum = read_u16(elf, 56)?;
    ensure!(
        phentsize >= 56 && phnum > 0 && phnum <= 128,
        "kernel ELF has an invalid program-header table"
    );
    let mut segments = Vec::new();
    let mut index = 0u16;
    while index < phnum {
        let offset = phoff
            .checked_add(u64::from(index).saturating_mul(u64::from(phentsize)))
            .ok_or_else(|| anyhow::anyhow!("kernel program-header offset overflow"))?;
        let offset = usize::try_from(offset)
            .map_err(|_| anyhow::anyhow!("kernel program-header offset is too large"))?;
        let kind = read_u32(elf, offset)?;
        if kind == 1 {
            let segment = LoadSegment {
                offset: read_u64(elf, offset + 8)?,
                paddr: read_u64(elf, offset + 24)?,
                filesz: read_u64(elf, offset + 32)?,
                memsz: read_u64(elf, offset + 40)?,
            };
            ensure!(
                segment.memsz != 0,
                "kernel PT_LOAD segment has no memory size"
            );
            segments.push(segment);
        }
        index = index.saturating_add(1);
    }
    segments.sort_by_key(|segment| segment.paddr);
    Ok(segments)
}

fn read_u16(bytes: &[u8], offset: usize) -> Result<u16> {
    let end = offset
        .checked_add(2)
        .ok_or_else(|| anyhow::anyhow!("ELF read offset overflow"))?;
    let slice = bytes
        .get(offset..end)
        .ok_or_else(|| anyhow::anyhow!("kernel ELF is truncated"))?;
    Ok(u16::from_le_bytes([slice[0], slice[1]]))
}

fn read_u32(bytes: &[u8], offset: usize) -> Result<u32> {
    let end = offset
        .checked_add(4)
        .ok_or_else(|| anyhow::anyhow!("ELF read offset overflow"))?;
    let slice = bytes
        .get(offset..end)
        .ok_or_else(|| anyhow::anyhow!("kernel ELF is truncated"))?;
    Ok(u32::from_le_bytes([slice[0], slice[1], slice[2], slice[3]]))
}

fn read_u64(bytes: &[u8], offset: usize) -> Result<u64> {
    let end = offset
        .checked_add(8)
        .ok_or_else(|| anyhow::anyhow!("ELF read offset overflow"))?;
    let slice = bytes
        .get(offset..end)
        .ok_or_else(|| anyhow::anyhow!("kernel ELF is truncated"))?;
    let mut value = [0u8; 8];
    value.copy_from_slice(slice);
    Ok(u64::from_le_bytes(value))
}

/// Build the FAT boot partition.
fn build_esp(kernel8: &[u8], firmware: &Path) -> Result<Vec<u8>> {
    let size = usize::try_from(ESP_SECTORS * SECTOR_SIZE)?;
    let mut fat = FatImageBuilder::new(size)?.with_epoch(source_date_epoch());
    fat.add_file("config.txt", CONFIG_TXT.as_bytes())?;
    fat.add_file("kernel8.img", kernel8)?;
    for name in FIRMWARE_FILES {
        let path = firmware.join(name);
        let bytes = fs::read(&path).with_context(|| {
            format!(
                "missing Raspberry Pi firmware file {} (set RAVEN_RPI_FIRMWARE to the firmware boot directory)",
                path.display()
            )
        })?;
        fat.add_file(name, &bytes)?;
    }
    let esp = fat.finish()?;
    ensure!(
        esp.len() == size,
        "Pi boot partition has the wrong size ({} bytes, expected {size})",
        esp.len()
    );
    Ok(esp)
}

/// Locate the read-only Pi firmware boot directory.
fn firmware_dir(root: &Path) -> Result<PathBuf> {
    if let Some(value) = std::env::var_os("RAVEN_RPI_FIRMWARE") {
        let directory = PathBuf::from(value);
        if directory.is_dir() {
            return Ok(directory);
        }
        bail!(
            "RAVEN_RPI_FIRMWARE does not name a directory: {}",
            directory.display()
        );
    }
    // The clone lives beside `ravenbsd-resources` in the workspace, or under `minions/<name>/`
    // in a minion checkout; search the nearest ancestors rather than baking a host path in.
    let mut last = root.to_path_buf();
    for ancestor in root.ancestors().skip(1).take(4) {
        let directory = ancestor.join("ravenbsd-resources/rpi-firmware/boot");
        if directory.is_dir() {
            return Ok(directory);
        }
        last = directory;
    }
    bail!(
        "Raspberry Pi firmware not found (last checked {}); run `make setup` or set RAVEN_RPI_FIRMWARE to the firmware boot directory",
        last.display()
    )
}

fn resolve_override(root: &Path, path: &Path) -> PathBuf {
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        root.join(path)
    }
}

fn resolve_output(root: &Path, output: Option<&Path>) -> PathBuf {
    match output {
        Some(path) => resolve_override(root, path),
        None => root
            .join("build")
            .join(Arch::Aarch64.as_str())
            .join("ravenbsd-pi4.img"),
    }
}

const _: () = {
    assert!(PI_LOAD == 0x80000);
    assert!(!CONFIG_TXT.is_empty());
};

#[cfg(test)]
#[path = "pi_ut.rs"]
mod tests;
