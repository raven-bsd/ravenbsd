// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Raw GPT image assembly: loader, kernel and initfs into a FAT32 ESP.

use std::fs::{self, File, OpenOptions};
use std::io::{Cursor, Read, Seek, SeekFrom, Write};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;

use fatfs::{FileSystem, FsOptions};

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

pub(crate) const IMAGE_BYTES: u64 = 320 * 1024 * 1024;
pub(crate) const TOTAL_SECTORS: u64 = IMAGE_BYTES / SECTOR_SIZE;
pub(crate) const ESP_START_LBA: u64 = 2048;
pub(crate) const ESP_SECTORS: u64 = 220 * 1024 * 1024 / SECTOR_SIZE;
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

/// Bootstrap initfs profile for a persistent UFS-root image.
///
/// A profile opts into the qemu-ufs-style payload by shipping a rootfs spec and a sibling
/// `<name>-boot.toml` bootstrap profile. The image builder no longer keys on the profile name.
fn ufs_boot_profile(root: &Path, profile_name: &str) -> Option<String> {
    crate::rootfs::spec_path(root, profile_name)?;
    let boot = root
        .join("profiles")
        .join(format!("{profile_name}-boot.toml"));
    boot.is_file().then(|| format!("{profile_name}-boot"))
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
    // A release image must embed the kernel built from the clean checked-out HEAD. Reject a
    // dirty tree up front and a kernel override, which would bypass that guarantee. The profile
    // decides the build mode: release image names default to release, and `[image] release`
    // is the explicit switch for demo/test profiles.
    let profile = Profile::load(root, profile_name)?;
    let release_image = build::image_build_release(arch, &profile);
    let ufs_boot = ufs_boot_profile(root, profile_name);
    let qemu_ufs = profile_name == "qemu-ufs";
    let pi4_pivot = profile_name == "pi4-root-pivot";
    if release_image {
        build::require_clean_release_head(root, profile_name)?;
        if overrides.kernel.is_some() {
            bail!(
                "{profile_name} image build does not accept a --kernel override: the release image must embed the kernel built from HEAD"
            );
        }
    }
    if overrides.initfs.is_none() {
        crate::addons::ensure_current(root, arch, &profile)?;
    }

    if platform == Platform::Pi4 {
        if crate::rootfs::spec_path(root, profile_name).is_some() {
            return build_pi4_sd(root, arch, profile_name, cmdline, overrides, release_image);
        }
        let release_kernel = if release_image {
            Some(build::run_pi4_kernel(root, true)?)
        } else {
            None
        };
        let code = crate::pi::build(
            root,
            arch,
            profile_name,
            cmdline,
            overrides.initfs,
            release_kernel.as_deref().or(overrides.kernel),
            overrides.output,
        )?;
        if code == 0 && release_image {
            let kernel = release_kernel
                .as_deref()
                .ok_or_else(|| anyhow::anyhow!("release Pi kernel path is missing"))?;
            build::verify_artifact_profile(kernel, true)?;
            build::verify_release_kernel(root, kernel, profile_name)?;
        }
        return Ok(code);
    }
    let workspace = Workspace::load(root)?;

    // The loader is always needed. A test kernel override replaces the real kernel, so there is
    // no reason to build it.
    let code = build::run(
        &workspace,
        Component::Loader,
        arch,
        release_image,
        &[],
        false,
    )?;
    if code != 0 {
        bail!("loader build failed with exit code {code}");
    }
    if overrides.kernel.is_none() {
        let code = build::run(
            &workspace,
            Component::Kernel,
            arch,
            release_image,
            &[],
            false,
        )?;
        if code != 0 {
            bail!("kernel build failed with exit code {code}");
        }
        let kernel = build::profile_artifact_path(root, arch, Component::Kernel, release_image);
        build::verify_artifact_profile(&kernel, release_image)?;
        if release_image {
            build::verify_release_kernel(root, &kernel, profile_name)?;
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
        None if rootfs && ufs_boot.is_none() && !pi4_pivot => {
            build_release_initfs(root, arch, &profile, &workspace)?
        }
        None => {
            // K176: the UFS-root image keeps the initfs bootstrap-only; the release
            // userland goes into the root disk below.
            let initfs_profile = if let Some(boot) = &ufs_boot {
                boot.as_str()
            } else if pi4_pivot {
                "pi4-root-pivot-boot"
            } else {
                profile_name
            };
            let path = crate::initfs::build(root, arch, initfs_profile)?;
            fs::read(&path).with_context(|| format!("cannot read {}", path.display()))?
        }
    };
    let kernel_path = match overrides.kernel {
        Some(path) => resolve_override(root, path),
        None => build::profile_artifact_path(root, arch, Component::Kernel, release_image),
    };

    let loader = build::profile_artifact_path(root, arch, Component::Loader, release_image);
    build::verify_artifact_profile(&loader, release_image)?;
    let loader_bytes =
        fs::read(&loader).with_context(|| format!("cannot read {}", loader.display()))?;
    let kernel_bytes =
        fs::read(&kernel_path).with_context(|| format!("cannot read {}", kernel_path.display()))?;
    // K193: the bootable UFS demo is one GPT disk (ESP first, UFS2 second). Keep the
    // two-disk preflight artifacts for `ufs-root`: its bootstrap initfs still selects
    // partition 1 of the separate root disk.
    let mut demo_ufs: Option<Vec<u8>> = None;
    let mut preflight_initfs: Option<Vec<u8>> = None;
    // The `hw` profile carries a UFS root. Until virtio-blk lands, blockd serves it from the
    // initfs as a RAM disk, so the one-partition GPT disk is inserted at /etc/blockd.img.
    let initfs_bytes = if pi4_pivot {
        // K182: the Pi 4 fake card loads the GPT/UFS image from the initfs. Build the
        // release payload with the Pi 4 profile and wrap it in the same one-partition
        // GPT disk the emmc2 test backend serves.
        crate::initfs::ensure_prerequisites(root, arch, &profile)?;
        let ufs = crate::rootfs::build_sd_root(root, arch, profile_name, true)?;
        let disk = root
            .join("build")
            .join(arch.as_str())
            .join("pi4-root-pivot-root.img");
        crate::rootfs::build_root_disk(&ufs, &disk)?;
        let data = fs::read(&disk).with_context(|| format!("cannot read {}", disk.display()))?;
        crate::rootfs::inject_initfs_file(&initfs_bytes, "/etc/pi4-root.img", &data, 0o444)?
    } else if ufs_boot.is_some() {
        // The payload profile stages brush/uutils and other generated files; build those
        // prerequisites before staging the UFS tree.
        crate::initfs::ensure_prerequisites(root, arch, &profile)?;
        // The two-disk preflight keeps a real one-partition GPT/UFS2 root disk.
        let ufs_path = crate::rootfs::build_sd_root(root, arch, profile_name, true)?;
        let disk = root
            .join("build")
            .join(arch.as_str())
            .join(format!("{profile_name}-root.img"));
        crate::rootfs::build_root_disk(&ufs_path, &disk)?;
        let ufs =
            fs::read(&ufs_path).with_context(|| format!("cannot read {}", ufs_path.display()))?;
        demo_ufs = Some(ufs);
        preflight_initfs = Some(initfs_bytes.clone());
        if qemu_ufs {
            // The one-disk demo has the ESP as partition 1 and the UFS root as partition 2.
            // The two-disk preflight keeps the bootstrap's partition-1 fsd manifest.
            let manifest = fs::read(root.join("profiles/qemu-ufs.d/fsd-ufs-demo.manifest"))
                .context("cannot read the demo fsd-ufs manifest")?;
            let preflight = preflight_initfs
                .as_deref()
                .ok_or_else(|| anyhow::anyhow!("{profile_name} preflight initfs is missing"))?;
            replace_initfs_file(
                preflight,
                "/etc/services.d/fsd-ufs.toml",
                &manifest,
                0o644,
                crate::initfs::image_epoch(root),
            )?
        } else {
            initfs_bytes
        }
    } else {
        match crate::rootfs::build_for_profile(root, arch, profile_name, true)? {
            Some(disk) => {
                let data =
                    fs::read(&disk).with_context(|| format!("cannot read {}", disk.display()))?;
                crate::rootfs::inject_initfs_file(&initfs_bytes, "/etc/blockd.img", &data, 0o444)?
            }
            None => initfs_bytes,
        }
    };

    let boot_toml = boot_toml(&profile, cmdline);
    let loader_name = match arch {
        Arch::X86_64 => "EFI/BOOT/BOOTX64.EFI",
        Arch::Aarch64 => "EFI/BOOT/BOOTAA64.EFI",
        Arch::Riscv64 => bail!("loader is not supported for riscv64"),
    };

    let esp = build_esp(
        loader_name,
        &loader_bytes,
        &kernel_bytes,
        &initfs_bytes,
        boot_toml.as_bytes(),
    )?;

    let output = overrides
        .output
        .map(Path::to_path_buf)
        .unwrap_or_else(|| default_output(root, arch));
    if let Some(parent) = output.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("cannot create {}", parent.display()))?;
    }

    if let (Some(ufs), Some(preflight)) = (demo_ufs.as_deref(), preflight_initfs.as_deref()) {
        let preflight_esp = build_esp(
            loader_name,
            &loader_bytes,
            &kernel_bytes,
            preflight,
            boot_toml.as_bytes(),
        )?;
        let preflight_gpt = preflight_gpt()?;
        // `--output` remains the two-disk preflight ESP (K176). The one-disk demo is
        // emitted only at its fixed build/<arch>/ravenbsd-ufs-demo path.
        write_image(&output, &preflight_gpt, &preflight_esp)?;
        if qemu_ufs {
            write_ufs_demo(root, arch, &esp, ufs)?;
        }
        println!("{}", output.display());
        return Ok(0);
    }

    let gpt = preflight_gpt()?;
    write_image(&output, &gpt, &esp)?;
    println!("{}", output.display());
    Ok(0)
}

fn preflight_gpt() -> Result<GptLayout> {
    GptLayout::new(
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
    )
}

fn build_esp(
    loader_name: &str,
    loader: &[u8],
    kernel: &[u8],
    initfs: &[u8],
    boot_toml: &[u8],
) -> Result<Vec<u8>> {
    let mut fat = FatImageBuilder::new(usize::try_from(ESP_SECTORS * SECTOR_SIZE)?)?
        .with_epoch(source_date_epoch());
    for dir in ["EFI", "EFI/BOOT", "raven"] {
        fat.add_dir(dir)?;
    }
    fat.add_file(loader_name, loader)?;
    fat.add_file("raven/kernel.elf", kernel)?;
    fat.add_file("raven/initfs.img", initfs)?;
    fat.add_file("raven/boot.toml", boot_toml)?;
    let esp = fat.finish()?;
    ensure!(
        esp.len() as u64 == ESP_SECTORS * SECTOR_SIZE,
        "ESP image has the wrong size"
    );
    Ok(esp)
}

/// Write the one-disk demo image and its qcow2 + run.sh/README artifacts.
fn write_ufs_demo(root: &Path, arch: Arch, esp: &[u8], ufs: &[u8]) -> Result<()> {
    ensure!(
        !ufs.is_empty() && ufs.len().is_multiple_of(SECTOR_SIZE as usize),
        "UFS payload is not a whole number of 512-byte sectors"
    );
    let ufs_sectors = ufs.len() as u64 / SECTOR_SIZE;
    let ufs_start = ESP_START_LBA + ESP_SECTORS;
    let total = ufs_start
        .checked_add(ufs_sectors)
        .and_then(|value| value.checked_add(GPT_ENTRY_SECTORS + 1))
        .ok_or_else(|| anyhow::anyhow!("UFS demo image size overflow"))?;
    let gpt = GptLayout::new(
        total,
        vec![
            Partition::new(
                TYPE_ESP,
                0,
                ESP_START_LBA,
                ESP_START_LBA + ESP_SECTORS - 1,
                "ESP",
            ),
            Partition::new(
                crate::rootfs::TYPE_FREEBSD_UFS,
                1,
                ufs_start,
                ufs_start + ufs_sectors - 1,
                "ufs-root",
            ),
        ],
    )?;
    let demo = root
        .join("build")
        .join(arch.as_str())
        .join("ravenbsd-ufs-demo");
    fs::create_dir_all(&demo).with_context(|| format!("cannot create {}", demo.display()))?;
    let raw = demo.join("ravenbsd-ufs-demo.raw");
    write_sd_image(&raw, &gpt, esp, ufs_start, ufs)?;

    let qcow2 = demo.join("ravenbsd-ufs-demo.qcow2");
    let status = Command::new("qemu-img")
        .arg("convert")
        .arg("-O")
        .arg("qcow2")
        .arg(&raw)
        .arg(&qcow2)
        .status()
        .context("cannot run qemu-img; install qemu-utils")?;
    ensure!(status.success(), "qemu-img convert failed ({status})");
    let _ = fs::remove_file(&raw);
    let run = demo.join("run.sh");
    let readme = demo.join("README");
    fs::copy(root.join("scripts/qemu-ufs-run.sh"), &run)
        .context("cannot copy the demo run.sh template")?;
    fs::copy(root.join("scripts/qemu-ufs-README.md"), &readme)
        .context("cannot copy the demo README template")?;
    fs::set_permissions(&run, fs::Permissions::from_mode(0o755))?;
    println!("{}", qcow2.display());
    Ok(())
}

fn replace_initfs_file(
    image: &[u8],
    path: &str,
    data: &[u8],
    mode: u32,
    epoch: u64,
) -> Result<Vec<u8>> {
    let parsed = raven_initfs::Initfs::parse(image)
        .map_err(|error| anyhow::anyhow!("cannot parse initfs: {error:?}"))?;
    let mut builder = raven_initfs::InitfsBuilder::new().with_image_epoch(epoch);
    for entry in parsed.entries() {
        if entry.path == path {
            builder.add_file(path, data, mode)?;
        } else {
            builder.add_file(entry.path, entry.data, entry.mode)?;
        }
    }
    Ok(builder.build())
}

/// Build the initfs for a rootfs profile from release user programs.
///
/// The normal image path delegates to `initfs::build`, which resolves the profile's
/// `[initfs] release` switch and the `RAVEN_USER_RELEASE` override. A rootfs profile is a
/// production image, so this path keeps the unconditional release build.
fn build_release_initfs(
    root: &Path,
    arch: Arch,
    profile: &Profile,
    workspace: &Workspace,
) -> Result<Vec<u8>> {
    crate::initfs::ensure_prerequisites(root, arch, profile)?;
    let mut packages: Vec<String> = profile
        .initfs
        .programs
        .iter()
        .map(|program| program.package.clone())
        .collect();
    packages.sort();
    packages.dedup();
    if !packages.is_empty() {
        let code = build::run(workspace, Component::User, arch, true, &packages, false)?;
        if code != 0 {
            bail!("release user program build failed with exit code {code}");
        }
    }
    let mut builder =
        raven_initfs::InitfsBuilder::new().with_image_epoch(crate::initfs::image_epoch(root));
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
        let source = crate::initfs::source_path(root, arch, &file.src);
        let data = fs::read(&source).with_context(|| {
            format!(
                "cannot read profile file {} for initfs path {} (resolved {})",
                file.src,
                file.path,
                source.display()
            )
        })?;
        crate::initfs::verify_staged_elf(arch, &source, &data)
            .with_context(|| format!("initfs path {}", file.path))?;
        builder.add_file(&file.path, &data, file.mode)?;
    }
    let image = builder.build();
    crate::initfs::verify_dynamic_payload(&image, root, arch, profile.initfs.dynamic)?;
    Ok(image)
}

fn build_pi4_sd(
    root: &Path,
    arch: Arch,
    profile_name: &str,
    cmdline: Option<&str>,
    overrides: Overrides<'_>,
    release_image: bool,
) -> Result<u8> {
    ensure!(
        arch == Arch::Aarch64,
        "the pi4 platform requires --arch aarch64"
    );
    let profile = Profile::load(root, profile_name)?;
    let release_kernel = if release_image {
        Some(build::run_pi4_kernel(root, true)?)
    } else {
        None
    };
    let temp = root
        .join("build")
        .join(arch.as_str())
        .join("pi4-rootfs-boot.img");
    if let Some(parent) = temp.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("cannot create {}", parent.display()))?;
    }

    let code = crate::pi::build(
        root,
        arch,
        profile_name,
        cmdline,
        overrides.initfs,
        release_kernel.as_deref().or(overrides.kernel),
        Some(&temp),
    )?;
    if code != 0 {
        bail!("Pi 4 boot-partition build failed with exit code {code}");
    }
    if release_image {
        let kernel = release_kernel
            .as_deref()
            .ok_or_else(|| anyhow::anyhow!("release Pi kernel path is missing"))?;
        build::verify_artifact_profile(kernel, true)?;
        build::verify_release_kernel(root, kernel, profile_name)?;
    }

    let fat_offset = ESP_START_LBA * SECTOR_SIZE;
    let fat_length = ESP_SECTORS * SECTOR_SIZE;
    let fat = read_at(&temp, fat_offset, fat_length)?;
    let cmdline = cmdline.unwrap_or(&profile.boot.cmdline);
    let fat = fat_with_cmdline(&fat, cmdline, source_date_epoch())?;

    let ufs_path = crate::rootfs::build_sd_root(root, arch, profile_name, true)?;
    let ufs = fs::read(&ufs_path).with_context(|| format!("cannot read {}", ufs_path.display()))?;
    ensure!(
        !ufs.is_empty() && ufs.len().is_multiple_of(SECTOR_SIZE as usize),
        "{} is not a whole number of 512-byte sectors",
        ufs_path.display()
    );
    let ufs_sectors = ufs.len() as u64 / SECTOR_SIZE;
    let ufs_start = ESP_START_LBA + ESP_SECTORS;
    let total = ufs_start
        .checked_add(ufs_sectors)
        .and_then(|value| value.checked_add(GPT_ENTRY_SECTORS + 1))
        .ok_or_else(|| anyhow::anyhow!("Pi 4 SD image size overflow"))?;
    let gpt = GptLayout::new(
        total,
        vec![
            Partition::new(
                TYPE_ESP,
                0,
                ESP_START_LBA,
                ESP_START_LBA + ESP_SECTORS - 1,
                "BOOT",
            ),
            Partition::new(
                crate::rootfs::TYPE_FREEBSD_UFS,
                1,
                ufs_start,
                ufs_start + ufs_sectors - 1,
                "ROOT",
            ),
        ],
    )?;

    let output = overrides.output.map(Path::to_path_buf).unwrap_or_else(|| {
        root.join("build")
            .join(arch.as_str())
            .join("ravenbsd-pi4-sd.img")
    });
    if let Some(parent) = output.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("cannot create {}", parent.display()))?;
    }
    write_sd_image(&output, &gpt, &fat, ufs_start, &ufs)?;
    let _ = fs::remove_file(&temp);
    println!("{}", output.display());
    Ok(0)
}

fn read_at(path: &Path, offset: u64, length: u64) -> Result<Vec<u8>> {
    let mut file = File::open(path).with_context(|| format!("cannot open {}", path.display()))?;
    file.seek(SeekFrom::Start(offset))?;
    let length = usize::try_from(length).map_err(|_| anyhow::anyhow!("read is too large"))?;
    let mut data = vec![0u8; length];
    file.read_exact(&mut data)
        .with_context(|| format!("cannot read {}", path.display()))?;
    Ok(data)
}

fn fat_with_cmdline(esp: &[u8], cmdline: &str, epoch: Option<i64>) -> Result<Vec<u8>> {
    let mut buffer = esp.to_vec();
    let mut dirs = Vec::new();
    let mut files = Vec::new();
    {
        let cursor = Cursor::new(buffer.as_mut_slice());
        let filesystem = FileSystem::new(cursor, FsOptions::new().update_accessed_date(false))?;
        let root = filesystem.root_dir();
        collect_fat_dir(&root, "", &mut dirs, &mut files)?;
        drop(root);
        drop(filesystem);
    }
    let mut builder = FatImageBuilder::new(esp.len())?.with_epoch(epoch);
    for dir in dirs {
        builder.add_dir(&dir)?;
    }
    for (path, data) in files {
        builder.add_file(&path, &data)?;
    }
    builder.add_file("cmdline.txt", format!("{cmdline}\n").as_bytes())?;
    builder.finish()
}

fn collect_fat_dir(
    dir: &fatfs::Dir<'_, Cursor<&mut [u8]>>,
    prefix: &str,
    dirs: &mut Vec<String>,
    files: &mut Vec<(String, Vec<u8>)>,
) -> Result<()> {
    for entry in dir.iter() {
        let entry = entry?;
        let name = entry.file_name();
        if name == "." || name == ".." {
            continue;
        }
        let path = if prefix.is_empty() {
            name.clone()
        } else {
            format!("{prefix}/{name}")
        };
        if entry.is_dir() {
            dirs.push(path.clone());
            let child = entry.to_dir();
            collect_fat_dir(&child, &path, dirs, files)?;
        } else if entry.is_file() {
            let mut file = entry.to_file();
            let mut data = Vec::new();
            file.read_to_end(&mut data)?;
            files.push((path, data));
        }
    }
    Ok(())
}

fn write_sd_image(
    path: &Path,
    gpt: &GptLayout,
    fat: &[u8],
    ufs_start: u64,
    ufs: &[u8],
) -> Result<()> {
    let total = gpt.total_sectors();
    let mut file = OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .open(path)
        .with_context(|| format!("cannot create {}", path.display()))?;
    file.set_len(total * SECTOR_SIZE)
        .with_context(|| format!("cannot size {}", path.display()))?;
    write_at(&mut file, 0, &gpt.protective_mbr())?;
    write_at(&mut file, SECTOR_SIZE, &gpt.primary_header())?;
    write_at(&mut file, 2 * SECTOR_SIZE, &gpt.entry_array())?;
    let backup_entries_lba = total - 1 - GPT_ENTRY_SECTORS;
    write_at(
        &mut file,
        backup_entries_lba * SECTOR_SIZE,
        &gpt.entry_array(),
    )?;
    write_at(&mut file, (total - 1) * SECTOR_SIZE, &gpt.backup_header())?;
    write_at(&mut file, ESP_START_LBA * SECTOR_SIZE, fat)?;
    write_at(&mut file, ufs_start * SECTOR_SIZE, ufs)?;
    file.flush()
        .with_context(|| format!("cannot flush {}", path.display()))?;
    Ok(())
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
