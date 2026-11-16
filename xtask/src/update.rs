// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! A/B image update planner (K27).
//!
//! The planner keeps the confirmed slot, installs the inactive slot, records the trial budget in
//! `boot.toml` and rewrites the GPT with two root partitions. The loader reads the same metadata
//! and rolls back when the trial image does not load.
//!
//! The QEMU fixture is a single FAT volume with `raven/slot-a` and `raven/slot-b`; the harness
//! attaches it as a second disk so the normal image builder does not need an A/B profile.

use std::fs::{self, File};
use std::io::{Cursor, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use anyhow::{Context, Result, anyhow, bail, ensure};
use fatfs::{FileSystem as FatFileSystem, FsOptions};
use raven_initfs::{Initfs, InitfsBuilder};

use crate::build::{self, Arch, Component};
use crate::fat::FatImageBuilder;
use crate::gpt::{GptLayout, Partition, SECTOR_SIZE, TYPE_ESP, TYPE_LINUX_DATA};
use crate::image::{self, Overrides, Platform};
use crate::profiles::Profile;
use crate::repo::Workspace;

const FIXTURE_CMDLINE: &str = "console=ttyS0 test-exit=1";
/// The Pi firmware loads the flat image at this physical address.
const PI_LOAD: u64 = 0x80000;
/// The flat kernel image the Pi firmware boots from the FAT root.
const PI4_KERNEL: &str = "kernel8.img";

/// One root slot.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Slot {
    A,
    B,
}

impl Slot {
    fn parse(value: &str) -> Option<Self> {
        match value {
            "a" => Some(Self::A),
            "b" => Some(Self::B),
            _ => None,
        }
    }

    const fn as_str(self) -> &'static str {
        match self {
            Self::A => "a",
            Self::B => "b",
        }
    }

    const fn other(self) -> Self {
        match self {
            Self::A => Self::B,
            Self::B => Self::A,
        }
    }
}

/// Fixture health marker state.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum FixtureHealth {
    /// Marker confirms the trial slot.
    Good,
    /// Marker is stale but the trial slot is valid; the loader boots it and waits.
    Trial,
    /// Marker is stale and the trial slot is invalid; the loader rolls back.
    Bad,
}

/// Parsed `update` options.
#[derive(Clone, Debug)]
struct Options {
    arch: Arch,
    platform: Platform,
    profile: String,
    image: Option<PathBuf>,
    kernel: Option<PathBuf>,
    initfs: Option<PathBuf>,
    base_kernel: Option<PathBuf>,
    base_initfs: Option<PathBuf>,
    slot: Option<Slot>,
    attempts: u32,
    fixture: Option<PathBuf>,
    health: Option<FixtureHealth>,
    blank: Option<PathBuf>,
    root_staging: Option<PathBuf>,
    root_size: u64,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            arch: Arch::X86_64,
            platform: Platform::Generic,
            profile: String::from("base"),
            image: None,
            kernel: None,
            initfs: None,
            base_kernel: None,
            base_initfs: None,
            slot: None,
            attempts: 2,
            fixture: None,
            health: None,
            blank: None,
            root_staging: None,
            root_size: 8 * 1024 * 1024,
        }
    }
}

/// `boot.toml` fields the planner preserves and rewrites.
#[derive(Clone, Debug, PartialEq, Eq)]
struct BootMeta {
    cmdline: String,
    log_level: String,
    slot: Slot,
    trial: Option<Slot>,
    trial_token: Option<u32>,
    attempts: u32,
    has_slot_config: bool,
}

impl Default for BootMeta {
    fn default() -> Self {
        Self {
            cmdline: String::new(),
            log_level: String::from("info"),
            slot: Slot::A,
            trial: None,
            trial_token: None,
            attempts: 0,
            has_slot_config: false,
        }
    }
}

impl BootMeta {
    fn render(&self) -> String {
        let mut out = format!(
            "cmdline = \"{}\"\nlog_level = \"{}\"\n",
            escape(&self.cmdline),
            escape(&self.log_level)
        );
        if self.has_slot_config {
            out.push_str(&format!("slot = \"{}\"\n", self.slot.as_str()));
            out.push_str(&format!(
                "trial = \"{}\"\n",
                self.trial.map_or("", Slot::as_str)
            ));
            out.push_str(&format!(
                "trial_token = \"{}\"\n",
                self.trial_token
                    .map_or_else(String::new, |token| format!("{token:08x}"))
            ));
            out.push_str(&format!("attempts = \"{}\"\n", self.attempts));
        }
        out
    }
}

/// Run the `xtask update` command.
pub(crate) fn run_cli(args: &[String]) -> ExitCode {
    match parse(args) {
        Ok(None) => ExitCode::SUCCESS,
        Ok(Some(options)) => match execute(&options) {
            Ok(()) => ExitCode::SUCCESS,
            Err(error) => {
                eprintln!("error: {error:#}");
                ExitCode::from(2)
            }
        },
        Err(error) => {
            eprintln!("error: {error:#}");
            ExitCode::from(2)
        }
    }
}

fn execute(options: &Options) -> Result<()> {
    if options.root_staging.is_some() && options.fixture.is_none() {
        bail!("--root-staging requires --fixture");
    }
    if let Some(path) = &options.blank {
        return build_blank(path);
    }
    if let Some(path) = &options.fixture {
        ensure!(
            options.platform == Platform::Generic,
            "--fixture supports only the generic platform"
        );
        return build_fixture(options, path);
    }
    let current = std::env::current_dir().context("cannot get the current directory")?;
    let root = crate::repo::find_root(&current)?;
    update_image(&root, options)
}

fn parse(args: &[String]) -> Result<Option<Options>> {
    if args.is_empty() || args.iter().any(|arg| arg == "-h" || arg == "--help") {
        print_help();
        return Ok(None);
    }

    let mut options = Options::default();
    let mut profile_set = false;
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--arch" => {
                index += 1;
                let value = args
                    .get(index)
                    .ok_or_else(|| anyhow!("--arch needs a value"))?;
                options.arch =
                    Arch::parse(value).ok_or_else(|| anyhow!("unknown architecture `{value}`"))?;
            }
            "--platform" => {
                index += 1;
                let value = args
                    .get(index)
                    .ok_or_else(|| anyhow!("--platform needs a value"))?;
                options.platform = Platform::parse(value)?;
            }
            "--profile" => {
                index += 1;
                options.profile = args
                    .get(index)
                    .ok_or_else(|| anyhow!("--profile needs a value"))?
                    .clone();
                profile_set = true;
            }
            "--image" => {
                index += 1;
                options.image = Some(PathBuf::from(
                    args.get(index)
                        .ok_or_else(|| anyhow!("--image needs a value"))?,
                ));
            }
            "--kernel" => {
                index += 1;
                options.kernel = Some(PathBuf::from(
                    args.get(index)
                        .ok_or_else(|| anyhow!("--kernel needs a value"))?,
                ));
            }
            "--initfs" => {
                index += 1;
                options.initfs = Some(PathBuf::from(
                    args.get(index)
                        .ok_or_else(|| anyhow!("--initfs needs a value"))?,
                ));
            }
            "--base-kernel" => {
                index += 1;
                options.base_kernel = Some(PathBuf::from(
                    args.get(index)
                        .ok_or_else(|| anyhow!("--base-kernel needs a value"))?,
                ));
            }
            "--base-initfs" => {
                index += 1;
                options.base_initfs = Some(PathBuf::from(
                    args.get(index)
                        .ok_or_else(|| anyhow!("--base-initfs needs a value"))?,
                ));
            }
            "--slot" => {
                index += 1;
                let value = args
                    .get(index)
                    .ok_or_else(|| anyhow!("--slot needs a value"))?;
                options.slot =
                    Some(Slot::parse(value).ok_or_else(|| anyhow!("--slot must be a or b"))?);
            }
            "--attempts" => {
                index += 1;
                let value = args
                    .get(index)
                    .ok_or_else(|| anyhow!("--attempts needs a value"))?;
                options.attempts = value
                    .parse()
                    .map_err(|_| anyhow!("--attempts must be an integer"))?;
                ensure!(options.attempts <= 255, "--attempts must be 0..=255");
            }
            "--fixture" => {
                index += 1;
                options.fixture = Some(PathBuf::from(
                    args.get(index)
                        .ok_or_else(|| anyhow!("--fixture needs a value"))?,
                ));
            }
            "--health" => {
                index += 1;
                let value = args
                    .get(index)
                    .ok_or_else(|| anyhow!("--health needs a value"))?;
                options.health = Some(match value.as_str() {
                    "good" => FixtureHealth::Good,
                    "trial" => FixtureHealth::Trial,
                    "bad" => FixtureHealth::Bad,
                    other => bail!("--health must be good, trial or bad, found `{other}`"),
                });
            }
            "--blank" => {
                index += 1;
                options.blank = Some(PathBuf::from(
                    args.get(index)
                        .ok_or_else(|| anyhow!("--blank needs a value"))?,
                ));
            }
            "--root-staging" => {
                index += 1;
                options.root_staging = Some(PathBuf::from(
                    args.get(index)
                        .ok_or_else(|| anyhow!("--root-staging needs a value"))?,
                ));
            }
            "--root-size" => {
                index += 1;
                let value = args
                    .get(index)
                    .ok_or_else(|| anyhow!("--root-size needs a value"))?;
                options.root_size = value
                    .parse()
                    .map_err(|_| anyhow!("--root-size must be an integer"))?;
                ensure!(
                    options.root_size >= 1024 * 1024 && options.root_size.is_multiple_of(4096),
                    "--root-size must be a 4 KiB multiple of at least 1 MiB"
                );
            }
            other => bail!("unknown update option `{other}`"),
        }
        index += 1;
    }
    ensure!(
        options.arch != Arch::Riscv64,
        "A/B updates are not supported for riscv64"
    );
    ensure!(
        options.platform != Platform::Pi4 || options.arch == Arch::Aarch64,
        "the pi4 platform requires --arch aarch64"
    );
    if !profile_set && options.platform == Platform::Pi4 {
        options.profile = String::from("pi4");
    }
    Ok(Some(options))
}

fn print_help() {
    println!(
        "RavenBSD A/B image update planner (K27)\n\
         \n\
         usage: cargo xtask update [options]\n\
         \n\
         options:\n\
           --arch <x86_64|aarch64>   architecture (default x86_64)\n\
           --platform <generic|pi4>  image layout (default generic)\n\
           --profile <name>          build profile (default base; pi4 for --platform pi4)\n\
           --image <path>            disk image to update or create\n\
           --kernel <path>           kernel for the inactive slot\n\
           --initfs <path>           initfs for the inactive slot\n\
           --base-kernel <path>      confirmed-slot kernel (fixture; default --kernel)\n\
           --base-initfs <path>      confirmed-slot initfs (fixture; default --initfs)\n\
           --slot <a|b>              inactive slot (default: the other slot)\n\
           --attempts <n>            trial boot budget (default 2)\n\
           --fixture <path>          build the QEMU rollback slot volume\n\
           --health <good|trial|bad> fixture health marker state\n\
           --root-staging <dir>      fixture UFS root staging tree\n\
           --root-size <bytes>       fixture UFS root size (default 8 MiB)\n\
           --blank <path>            build a blank GPT disk for the init fixture\n\
           --help                    show this help\n\
         \n\
         The planner writes both root slots, records slot/trial/attempts in\n\
         raven/boot.toml and leaves the confirmed slot untouched."
    );
}

fn update_image(root: &Path, options: &Options) -> Result<()> {
    let arch = options.arch;
    let image_path = options
        .image
        .clone()
        .unwrap_or_else(|| root.join(default_image_path(arch)));
    ensure!(
        arch != Arch::Riscv64,
        "A/B updates are not supported for riscv64"
    );
    if !image_path.is_file() {
        let code = image::run_with_overrides(
            root,
            arch,
            &options.profile,
            None,
            Overrides {
                output: Some(&image_path),
                ..Overrides::default()
            },
            options.platform,
        )?;
        ensure!(code == 0, "base image build failed with exit code {code}");
    }

    let mut esp = read_esp(&image_path)?;
    let layout = detect_layout(&mut esp)?;
    ensure!(
        layout == options.platform,
        "image {} is a {} image; pass --platform {}",
        image_path.display(),
        platform_name(layout),
        platform_name(options.platform)
    );
    if layout == Platform::Pi4 {
        return update_pi4_image(root, options, &image_path, esp);
    }

    let workspace = Workspace::load(root)?;
    let code = build::run(&workspace, Component::Loader, arch, false, &[])?;
    ensure!(code == 0, "loader build failed with exit code {code}");
    let loader_path_on_disk = build::artifact_path(root, arch, Component::Loader, "");
    let loader = fs::read(&loader_path_on_disk)
        .with_context(|| format!("cannot read {}", loader_path_on_disk.display()))?;

    let meta = read_meta(&mut esp, &options.profile, root)?;
    let legacy_kernel = fat_read(&mut esp, "raven/kernel.elf")?
        .ok_or_else(|| anyhow!("image has no raven/kernel.elf"))?;
    let legacy_initfs = fat_read(&mut esp, "raven/initfs.img")?
        .ok_or_else(|| anyhow!("image has no raven/initfs.img"))?;
    let mut slot_a_kernel =
        fat_read(&mut esp, "raven/slot-a/kernel.elf")?.unwrap_or(legacy_kernel.clone());
    let mut slot_a_initfs =
        fat_read(&mut esp, "raven/slot-a/initfs.img")?.unwrap_or(legacy_initfs.clone());
    let mut slot_b_kernel =
        fat_read(&mut esp, "raven/slot-b/kernel.elf")?.unwrap_or(legacy_kernel.clone());
    let mut slot_b_initfs =
        fat_read(&mut esp, "raven/slot-b/initfs.img")?.unwrap_or(legacy_initfs.clone());

    let (built_kernel, built_initfs) = if options.kernel.is_none() || options.initfs.is_none() {
        let (kernel, initfs) = build_latest_files(root, arch, &options.profile)?;
        (Some(kernel), Some(initfs))
    } else {
        (None, None)
    };
    let new_kernel = match &options.kernel {
        Some(path) => fs::read(path).with_context(|| format!("cannot read {}", path.display()))?,
        None => built_kernel.unwrap_or_else(|| legacy_kernel.clone()),
    };
    let new_initfs = match &options.initfs {
        Some(path) => fs::read(path).with_context(|| format!("cannot read {}", path.display()))?,
        None => built_initfs.unwrap_or_else(|| legacy_initfs.clone()),
    };

    let target = target_slot(&meta, options.slot);
    ensure!(
        target != meta.slot,
        "the inactive slot must differ from the confirmed slot"
    );
    let token = token_for(&new_kernel, &new_initfs);
    let new_initfs = inject_health(&new_initfs, target, token)?;
    match target {
        Slot::A => {
            slot_a_kernel = new_kernel;
            slot_a_initfs = new_initfs;
        }
        Slot::B => {
            slot_b_kernel = new_kernel;
            slot_b_initfs = new_initfs;
        }
    }

    let mut next_meta = meta;
    next_meta.trial = Some(target);
    next_meta.trial_token = Some(token);
    next_meta.attempts = options.attempts;
    next_meta.has_slot_config = true;

    let loader_path = loader_path(arch);
    let mut builder = FatImageBuilder::new(usize::try_from(image::ESP_SECTORS * SECTOR_SIZE)?)?
        .with_epoch(image::source_date_epoch());
    for dir in ["EFI", "EFI/BOOT", "raven", "raven/slot-a", "raven/slot-b"] {
        builder.add_dir(dir)?;
    }
    builder.add_file(loader_path, &loader)?;
    builder.add_file("raven/kernel.elf", &slot_a_kernel)?;
    builder.add_file("raven/initfs.img", &slot_a_initfs)?;
    builder.add_file("raven/slot-a/kernel.elf", &slot_a_kernel)?;
    builder.add_file("raven/slot-a/initfs.img", &slot_a_initfs)?;
    builder.add_file("raven/slot-b/kernel.elf", &slot_b_kernel)?;
    builder.add_file("raven/slot-b/initfs.img", &slot_b_initfs)?;
    builder.add_file("raven/boot.toml", next_meta.render().as_bytes())?;
    let esp = builder.finish()?;

    let gpt = two_root_gpt()?;
    image::write_image(&image_path, &gpt, &esp)?;
    println!("{}", image_path.display());
    println!(
        "update: confirmed slot {} trial slot {} attempts {}",
        next_meta.slot.as_str(),
        target.as_str(),
        next_meta.attempts
    );
    Ok(())
}

fn two_root_gpt() -> Result<GptLayout> {
    let root_start = image::ROOT_START_LBA;
    let root_sectors = image::ROOT_SECTORS;
    let second_start = root_start + root_sectors;
    let second_end = second_start + root_sectors - 1;
    ensure!(
        second_end < image::TOTAL_SECTORS,
        "the image has no room for two root slots"
    );
    GptLayout::new(
        image::TOTAL_SECTORS,
        vec![
            Partition::new(
                TYPE_ESP,
                0,
                image::ESP_START_LBA,
                image::ESP_START_LBA + image::ESP_SECTORS - 1,
                "ESP",
            ),
            Partition::new(
                TYPE_LINUX_DATA,
                1,
                root_start,
                root_start + root_sectors - 1,
                "ROOT-A",
            ),
            Partition::new(TYPE_LINUX_DATA, 2, second_start, second_end, "ROOT-B"),
        ],
    )
}

fn detect_layout(esp: &mut [u8]) -> Result<Platform> {
    if fat_read(esp, PI4_KERNEL)?.is_some() {
        return Ok(Platform::Pi4);
    }
    if fat_read(esp, "raven/kernel.elf")?.is_some() {
        return Ok(Platform::Generic);
    }
    bail!("image has neither {PI4_KERNEL} nor raven/kernel.elf")
}

const fn platform_name(platform: Platform) -> &'static str {
    match platform {
        Platform::Generic => "generic",
        Platform::Pi4 => "pi4",
    }
}

/// Install the inactive slot on a Pi 4 image.
///
/// The Pi firmware always boots `kernel8.img` from the FAT root, so the planner stages the A/B
/// metadata (`raven/boot.toml` and `raven/slot-*/kernel8.img`) and makes the root image the trial.
/// A Pi-side consumer of the metadata is not part of this ticket; board rollback is UNVERIFIED.
fn update_pi4_image(
    root: &Path,
    options: &Options,
    image_path: &Path,
    mut esp: Vec<u8>,
) -> Result<()> {
    ensure!(
        options.arch == Arch::Aarch64,
        "the pi4 platform requires --arch aarch64"
    );
    let meta = read_meta(&mut esp, &options.profile, root)?;
    let legacy =
        fat_read(&mut esp, PI4_KERNEL)?.ok_or_else(|| anyhow!("Pi 4 image has no {PI4_KERNEL}"))?;
    let mut slot_a =
        fat_read(&mut esp, "raven/slot-a/kernel8.img")?.unwrap_or_else(|| legacy.clone());
    let mut slot_b =
        fat_read(&mut esp, "raven/slot-b/kernel8.img")?.unwrap_or_else(|| legacy.clone());

    let new_initfs = match &options.initfs {
        Some(path) => fs::read(path).with_context(|| format!("cannot read {}", path.display()))?,
        None => build_latest_files(root, options.arch, &options.profile)?.1,
    };

    let target = target_slot(&meta, options.slot);
    ensure!(
        target != meta.slot,
        "the inactive slot must differ from the confirmed slot"
    );
    let (trial, token) = pi4_trial_image(root, options, &new_initfs, target)?;
    match target {
        Slot::A => slot_a = trial.clone(),
        Slot::B => slot_b = trial.clone(),
    }

    let mut next_meta = meta;
    next_meta.trial = Some(target);
    next_meta.trial_token = Some(token);
    next_meta.attempts = options.attempts;
    next_meta.has_slot_config = true;

    let esp = stage_pi4_esp(&mut esp, &next_meta, &slot_a, &slot_b, &trial)?;
    let gpt = two_root_gpt()?;
    image::write_image(image_path, &gpt, &esp)?;
    println!("{}", image_path.display());
    println!(
        "update: platform pi4 confirmed slot {} trial slot {} attempts {}",
        next_meta.slot.as_str(),
        target.as_str(),
        next_meta.attempts
    );
    Ok(())
}

/// Copy every file from the Pi boot partition except the planner-owned paths.
fn copy_fat_tree<T: fatfs::ReadWriteSeek>(
    dir: &fatfs::Dir<'_, T>,
    prefix: &str,
    builder: &mut FatImageBuilder,
    skip: &dyn Fn(&str) -> bool,
) -> Result<()> {
    for entry in dir.iter() {
        let entry = entry.context("cannot list a Pi 4 boot file")?;
        let name = entry.file_name();
        if name == "." || name == ".." {
            continue;
        }
        let path = if prefix.is_empty() {
            name
        } else {
            format!("{prefix}/{name}")
        };
        if skip(&path) {
            continue;
        }
        if entry.is_dir() {
            builder.add_dir(&path)?;
            copy_fat_tree(&entry.to_dir(), &path, builder, skip)?;
        } else if entry.is_file() {
            let mut file = entry.to_file();
            let mut data = Vec::new();
            file.read_to_end(&mut data)
                .with_context(|| format!("cannot read {path}"))?;
            builder.add_file(&path, &data)?;
        }
    }
    Ok(())
}

/// Rebuild the Pi boot partition with the preserved firmware files and the new slots.
fn stage_pi4_esp(
    esp: &mut [u8],
    next_meta: &BootMeta,
    slot_a: &[u8],
    slot_b: &[u8],
    trial: &[u8],
) -> Result<Vec<u8>> {
    let mut builder = FatImageBuilder::new(usize::try_from(image::ESP_SECTORS * SECTOR_SIZE)?)?
        .with_epoch(image::source_date_epoch());
    {
        let filesystem = FatFileSystem::new(Cursor::new(esp), FsOptions::new())
            .context("cannot read the Pi 4 boot partition")?;
        let root_dir = filesystem.root_dir();
        copy_fat_tree(&root_dir, "", &mut builder, &|path| {
            path == "raven" || path == PI4_KERNEL
        })?;
    }
    for dir in ["raven", "raven/slot-a", "raven/slot-b"] {
        builder.add_dir(dir)?;
    }
    builder.add_file("raven/boot.toml", next_meta.render().as_bytes())?;
    builder.add_file("raven/slot-a/kernel8.img", slot_a)?;
    builder.add_file("raven/slot-b/kernel8.img", slot_b)?;
    builder.add_file(PI4_KERNEL, trial)?;
    builder.finish()
}

/// Build the Pi 4 trial image through the normal Pi image builder and inject the health record.
///
/// The builder owns the Pi kernel build (`RAVEN_PI4=1`) and the flat `kernel8.img` layout; the
/// planner only replaces the trailing initfs with the health-injected one.
fn pi4_trial_image(
    root: &Path,
    options: &Options,
    initfs: &[u8],
    target: Slot,
) -> Result<(Vec<u8>, u32)> {
    let temp = root.join("build/update");
    fs::create_dir_all(&temp).with_context(|| format!("cannot create {}", temp.display()))?;
    let initfs_path = temp.join("pi4-trial-initfs.img");
    fs::write(&initfs_path, initfs)
        .with_context(|| format!("cannot write {}", initfs_path.display()))?;
    let output = temp.join("pi4-trial.img");
    let kernel = match &options.kernel {
        Some(path) => {
            let path = absolute_input(path)?;
            ensure_pi4_kernel(&fs::read(&path)?)?;
            Some(path)
        }
        None => None,
    };
    let code = crate::pi::build(
        root,
        Arch::Aarch64,
        &options.profile,
        None,
        Some(&initfs_path),
        kernel.as_deref(),
        Some(&output),
    )?;
    ensure!(
        code == 0,
        "Pi 4 trial image build failed with exit code {code}"
    );
    let mut esp = read_esp(&output)?;
    let flat = fat_read(&mut esp, PI4_KERNEL)?
        .ok_or_else(|| anyhow!("Pi 4 trial image has no {PI4_KERNEL}"))?;
    pi4_trial_from_flat(flat, initfs, target)
}

/// Anchor a CLI path at the current directory, matching the generic planner's reads.
fn absolute_input(path: &Path) -> Result<PathBuf> {
    if path.is_absolute() {
        Ok(path.to_path_buf())
    } else {
        Ok(std::env::current_dir()
            .context("cannot get the current directory")?
            .join(path))
    }
}

/// A Pi 4 kernel ELF enters at the firmware load address, unlike the UEFI aarch64 kernel.
fn ensure_pi4_kernel(elf: &[u8]) -> Result<()> {
    ensure!(elf.len() >= 32, "kernel ELF is shorter than the header");
    ensure!(&elf[..4] == b"\x7fELF", "kernel is not an ELF file");
    let mut entry = [0u8; 8];
    entry.copy_from_slice(&elf[24..32]);
    let entry = u64::from_le_bytes(entry);
    ensure!(
        entry == PI_LOAD,
        "kernel ELF entry {entry:#x} is not the Pi 4 load address {PI_LOAD:#x}; build it with RAVEN_PI4=1"
    );
    Ok(())
}

/// Replace the appended initfs with the health-injected one and derive the trial token.
fn pi4_trial_from_flat(flat: Vec<u8>, initfs: &[u8], target: Slot) -> Result<(Vec<u8>, u32)> {
    ensure!(
        flat.ends_with(initfs),
        "Pi 4 trial image does not carry the requested initfs"
    );
    let split = flat.len() - initfs.len();
    let token = token_for(&flat[..split], initfs);
    let mut trial = flat[..split].to_vec();
    trial.extend_from_slice(&inject_health(initfs, target, token)?);
    Ok((trial, token))
}

fn build_blank(output: &Path) -> Result<()> {
    let gpt = GptLayout::new(
        image::TOTAL_SECTORS,
        vec![Partition::new(
            TYPE_LINUX_DATA,
            0,
            image::ESP_START_LBA,
            image::ESP_START_LBA + image::ESP_SECTORS - 1,
            "ROOT",
        )],
    )?;
    let esp = vec![0u8; usize::try_from(image::ESP_SECTORS * SECTOR_SIZE)?];
    if let Some(parent) = output.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("cannot create {}", parent.display()))?;
    }
    image::write_image(output, &gpt, &esp)?;
    println!("{}", output.display());
    Ok(())
}

fn build_fixture(options: &Options, output: &Path) -> Result<()> {
    let kernel_path = options
        .kernel
        .as_ref()
        .ok_or_else(|| anyhow!("--fixture needs --kernel"))?;
    let trial_kernel =
        fs::read(kernel_path).with_context(|| format!("cannot read {}", kernel_path.display()))?;
    let trial_initfs = match &options.initfs {
        Some(path) => fs::read(path).with_context(|| format!("cannot read {}", path.display()))?,
        None => b"fixture initfs".to_vec(),
    };
    let base_kernel = match &options.base_kernel {
        Some(path) => fs::read(path).with_context(|| format!("cannot read {}", path.display()))?,
        None => trial_kernel.clone(),
    };
    let base_initfs = match &options.base_initfs {
        Some(path) => fs::read(path).with_context(|| format!("cannot read {}", path.display()))?,
        None => trial_initfs.clone(),
    };
    let token = token_for(&trial_kernel, &trial_initfs);
    let health = options.health.unwrap_or(FixtureHealth::Bad);
    let installed_kernel: &[u8] = match health {
        FixtureHealth::Good | FixtureHealth::Trial => &trial_kernel,
        FixtureHealth::Bad => b"not an ELF64 image\n",
    };
    let meta = BootMeta {
        cmdline: String::from(FIXTURE_CMDLINE),
        log_level: String::from("info"),
        slot: Slot::A,
        trial: Some(Slot::B),
        trial_token: Some(token),
        attempts: options.attempts,
        has_slot_config: true,
    };
    let mut builder = FatImageBuilder::new(usize::try_from(image::ESP_SECTORS * SECTOR_SIZE)?)?
        .with_epoch(image::source_date_epoch());
    for dir in ["raven", "raven/slot-a", "raven/slot-b"] {
        builder.add_dir(dir)?;
    }
    builder.add_file("raven/boot.toml", meta.render().as_bytes())?;
    builder.add_file("raven/slot-a/kernel.elf", &base_kernel)?;
    builder.add_file("raven/slot-a/initfs.img", &base_initfs)?;
    builder.add_file("raven/slot-b/kernel.elf", installed_kernel)?;
    builder.add_file("raven/slot-b/initfs.img", &trial_initfs)?;
    let esp = builder.finish()?;

    let (gpt, root_image) = if let Some(staging) = &options.root_staging {
        let staging = absolute_input(staging)?;
        ensure!(
            staging.is_dir(),
            "root staging tree {} is not a directory",
            staging.display()
        );
        write_root_health(&staging, token)?;
        let ufs_path = output.with_extension("root.ufs");
        crate::rootfs::build_ufs_root(&staging, options.root_size, &ufs_path)?;
        let ufs =
            fs::read(&ufs_path).with_context(|| format!("cannot read {}", ufs_path.display()))?;
        ensure!(
            ufs.len() as u64 == options.root_size,
            "fixture UFS root has the wrong size"
        );
        let _ = fs::remove_file(&ufs_path);
        let ufs_sectors = options.root_size / SECTOR_SIZE;
        let ufs_start = image::ESP_START_LBA + image::ESP_SECTORS;
        let ufs_end = ufs_start + ufs_sectors - 1;
        ensure!(
            ufs_end + 33 < image::TOTAL_SECTORS,
            "fixture UFS root does not fit the image"
        );
        let gpt = GptLayout::new(
            image::TOTAL_SECTORS,
            vec![
                Partition::new(
                    TYPE_ESP,
                    0,
                    image::ESP_START_LBA,
                    image::ESP_START_LBA + image::ESP_SECTORS - 1,
                    "ESP",
                ),
                Partition::new(TYPE_LINUX_DATA, 1, ufs_start, ufs_end, "ufs-root"),
            ],
        )?;
        (gpt, Some((ufs, ufs_start)))
    } else {
        let gpt = GptLayout::new(
            image::TOTAL_SECTORS,
            vec![Partition::new(
                TYPE_ESP,
                0,
                image::ESP_START_LBA,
                image::ESP_START_LBA + image::ESP_SECTORS - 1,
                "ESP",
            )],
        )?;
        (gpt, None)
    };
    if let Some(parent) = output.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("cannot create {}", parent.display()))?;
    }
    image::write_image(output, &gpt, &esp)?;
    if let Some((ufs, ufs_start)) = root_image {
        write_file_at(output, ufs_start * SECTOR_SIZE, &ufs)?;
    }
    let (marker_token, state) = match health {
        FixtureHealth::Good => (token, HEALTH_STATE_HEALTHY),
        FixtureHealth::Trial | FixtureHealth::Bad => (token, HEALTH_STATE_TRIAL),
    };
    write_marker(output, Slot::B, marker_token, state)?;
    println!("{}", output.display());
    Ok(())
}

/// Add the health record to a fixture UFS root staging tree.
fn write_root_health(staging: &Path, token: u32) -> Result<()> {
    let etc = staging.join("etc");
    fs::create_dir_all(&etc).with_context(|| format!("cannot create {}", etc.display()))?;
    let record = format!("slot = \"b\"\ntoken = \"{token:08x}\"\nlba = \"{MARKER_LBA}\"\n");
    let path = etc.join("boot-health");
    fs::write(&path, record).with_context(|| format!("cannot write {}", path.display()))
}

fn write_file_at(path: &Path, offset: u64, bytes: &[u8]) -> Result<()> {
    let mut file = fs::OpenOptions::new()
        .write(true)
        .open(path)
        .with_context(|| format!("cannot open {}", path.display()))?;
    file.seek(SeekFrom::Start(offset))?;
    file.write_all(bytes)?;
    file.flush()?;
    Ok(())
}

const HEALTH_STATE_HEALTHY: u8 = 1;
const HEALTH_STATE_TRIAL: u8 = 2;

fn write_marker(path: &Path, slot: Slot, token: u32, state: u8) -> Result<()> {
    let mut sector = [0u8; SECTOR_SIZE as usize];
    sector[..8].copy_from_slice(b"RAVENHLT");
    sector[8..10].copy_from_slice(&1u16.to_le_bytes());
    sector[10] = match slot {
        Slot::A => 0,
        Slot::B => 1,
    };
    sector[11] = state;
    sector[12..16].copy_from_slice(&token.to_le_bytes());
    let checksum = marker_checksum(&sector[..20]);
    sector[20..24].copy_from_slice(&checksum.to_le_bytes());
    let mut file = fs::OpenOptions::new()
        .write(true)
        .open(path)
        .with_context(|| format!("cannot open {}", path.display()))?;
    file.seek(SeekFrom::Start(MARKER_LBA * SECTOR_SIZE))?;
    file.write_all(&sector)?;
    file.flush()?;
    Ok(())
}

fn marker_checksum(bytes: &[u8]) -> u32 {
    let mut hash = 0x811c_9dc5u32;
    for byte in bytes {
        hash ^= u32::from(*byte);
        hash = hash.wrapping_mul(0x0100_0193);
    }
    hash
}

fn default_image_path(arch: Arch) -> PathBuf {
    PathBuf::from("build")
        .join(arch.as_str())
        .join(format!("ravenbsd-{}.img", arch.as_str()))
}

fn build_latest_files(root: &Path, arch: Arch, profile: &str) -> Result<(Vec<u8>, Vec<u8>)> {
    let base = root
        .join("build")
        .join("update")
        .join(format!("base-{}.img", arch.as_str()));
    let code = image::run_with_overrides(
        root,
        arch,
        profile,
        None,
        Overrides {
            output: Some(&base),
            ..Overrides::default()
        },
        Platform::Generic,
    )?;
    ensure!(code == 0, "base image build failed with exit code {code}");
    let mut esp = read_esp(&base)?;
    let kernel = fat_read(&mut esp, "raven/kernel.elf")?
        .ok_or_else(|| anyhow!("base image has no raven/kernel.elf"))?;
    let initfs = fat_read(&mut esp, "raven/initfs.img")?
        .ok_or_else(|| anyhow!("base image has no raven/initfs.img"))?;
    Ok((kernel, initfs))
}

fn token_for(kernel: &[u8], initfs: &[u8]) -> u32 {
    let mut hash = 0x811c_9dc5u32;
    for byte in kernel.iter().chain(initfs) {
        hash ^= u32::from(*byte);
        hash = hash.wrapping_mul(0x0100_0193);
    }
    hash
}

/// Add `/etc/boot-health` to the trial initfs so init can write the marker.
fn inject_health(initfs: &[u8], slot: Slot, token: u32) -> Result<Vec<u8>> {
    let parsed = Initfs::parse(initfs).context("trial initfs is not a valid initfs image")?;
    let mut builder = InitfsBuilder::new();
    for entry in parsed.entries() {
        if entry.path == "/etc/boot-health" {
            continue;
        }
        builder.add_file(entry.path, entry.data, entry.mode)?;
    }
    let text = format!(
        "slot = \"{}\"\ntoken = \"{token:08x}\"\nlba = \"{}\"\n",
        slot.as_str(),
        MARKER_LBA
    );
    builder.add_file("/etc/boot-health", text.as_bytes(), 0o444)?;
    Ok(builder.build())
}

const MARKER_LBA: u64 = 34;

fn target_slot(meta: &BootMeta, requested: Option<Slot>) -> Slot {
    requested.unwrap_or_else(|| meta.trial.map_or_else(|| meta.slot.other(), Slot::other))
}

fn loader_path(arch: Arch) -> &'static str {
    match arch {
        Arch::X86_64 => "EFI/BOOT/BOOTX64.EFI",
        Arch::Aarch64 => "EFI/BOOT/BOOTAA64.EFI",
        Arch::Riscv64 => "EFI/BOOT/BOOTRISCV64.EFI",
    }
}

fn read_esp(image_path: &Path) -> Result<Vec<u8>> {
    let mut file =
        File::open(image_path).with_context(|| format!("cannot open {}", image_path.display()))?;
    let mut esp = vec![0u8; usize::try_from(image::ESP_SECTORS * SECTOR_SIZE)?];
    file.seek(SeekFrom::Start(image::ESP_START_LBA * SECTOR_SIZE))?;
    file.read_exact(&mut esp)?;
    Ok(esp)
}

fn fat_read(esp: &mut [u8], path: &str) -> Result<Option<Vec<u8>>> {
    let filesystem = FatFileSystem::new(Cursor::new(esp), FsOptions::new())?;
    let root = filesystem.root_dir();
    let Ok(mut file) = root.open_file(path) else {
        return Ok(None);
    };
    let mut data = Vec::new();
    file.read_to_end(&mut data)?;
    Ok(Some(data))
}

fn read_meta(esp: &mut [u8], profile_name: &str, root: &Path) -> Result<BootMeta> {
    if let Some(bytes) = fat_read(esp, "raven/boot.toml")? {
        return parse_boot_meta(&bytes);
    }
    let profile = Profile::load(root, profile_name)?;
    Ok(BootMeta {
        cmdline: profile.boot.cmdline,
        log_level: profile.boot.log_level,
        ..BootMeta::default()
    })
}

fn parse_boot_meta(bytes: &[u8]) -> Result<BootMeta> {
    let text = std::str::from_utf8(bytes).context("boot.toml is not UTF-8")?;
    let mut meta = BootMeta::default();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let value = value.trim().trim_matches('"');
        match key.trim() {
            "cmdline" => meta.cmdline = value.to_string(),
            "log_level" => meta.log_level = value.to_string(),
            "slot" => {
                meta.slot = Slot::parse(value).ok_or_else(|| anyhow!("invalid slot `{value}`"))?;
                meta.has_slot_config = true;
            }
            "trial" => {
                meta.trial = Slot::parse(value);
                meta.has_slot_config = true;
            }
            "trial_token" => {
                meta.trial_token = if value.is_empty() {
                    None
                } else {
                    Some(
                        u32::from_str_radix(value, 16)
                            .map_err(|_| anyhow!("invalid trial_token `{value}`"))?,
                    )
                };
                meta.has_slot_config = true;
            }
            "attempts" => {
                meta.attempts = value
                    .parse()
                    .map_err(|_| anyhow!("invalid attempts `{value}`"))?;
                meta.has_slot_config = true;
            }
            _ => {}
        }
    }
    Ok(meta)
}

fn escape(value: &str) -> String {
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

#[cfg(test)]
#[path = "update_ut.rs"]
mod tests;
