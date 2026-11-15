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
use std::io::{Cursor, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use anyhow::{Context, Result, anyhow, bail, ensure};
use fatfs::{FileSystem as FatFileSystem, FsOptions};

use crate::build::{self, Arch, Component};
use crate::fat::FatImageBuilder;
use crate::gpt::{GptLayout, Partition, SECTOR_SIZE, TYPE_ESP, TYPE_LINUX_DATA};
use crate::image::{self, Overrides, Platform};
use crate::profiles::Profile;
use crate::repo::Workspace;

const FIXTURE_CMDLINE: &str = "console=ttyS0 test-exit=1";

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

/// Parsed `update` options.
#[derive(Clone, Debug)]
struct Options {
    arch: Arch,
    profile: String,
    image: Option<PathBuf>,
    kernel: Option<PathBuf>,
    initfs: Option<PathBuf>,
    slot: Option<Slot>,
    attempts: u32,
    fixture: Option<PathBuf>,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            arch: Arch::X86_64,
            profile: String::from("base"),
            image: None,
            kernel: None,
            initfs: None,
            slot: None,
            attempts: 2,
            fixture: None,
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
    if let Some(path) = &options.fixture {
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
            "--profile" => {
                index += 1;
                options.profile = args
                    .get(index)
                    .ok_or_else(|| anyhow!("--profile needs a value"))?
                    .clone();
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
            other => bail!("unknown update option `{other}`"),
        }
        index += 1;
    }
    ensure!(
        options.arch != Arch::Riscv64,
        "A/B updates are not supported for riscv64"
    );
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
           --profile <name>          build profile (default base)\n\
           --image <path>            disk image to update or create\n\
           --kernel <path>           kernel for the inactive slot\n\
           --initfs <path>           initfs for the inactive slot\n\
           --slot <a|b>              inactive slot (default: the other slot)\n\
           --attempts <n>            trial boot budget (default 2)\n\
           --fixture <path>          build the QEMU rollback slot volume\n\
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
            Platform::Generic,
        )?;
        ensure!(code == 0, "base image build failed with exit code {code}");
    }

    let workspace = Workspace::load(root)?;
    let code = build::run(&workspace, Component::Loader, arch, false, &[])?;
    ensure!(code == 0, "loader build failed with exit code {code}");
    let loader_path_on_disk = build::artifact_path(root, arch, Component::Loader, "");
    let loader = fs::read(&loader_path_on_disk)
        .with_context(|| format!("cannot read {}", loader_path_on_disk.display()))?;

    let mut esp = read_esp(&image_path)?;
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

    let new_kernel = match &options.kernel {
        Some(path) => fs::read(path).with_context(|| format!("cannot read {}", path.display()))?,
        None => legacy_kernel.clone(),
    };
    let new_initfs = match &options.initfs {
        Some(path) => fs::read(path).with_context(|| format!("cannot read {}", path.display()))?,
        None => legacy_initfs.clone(),
    };

    let target = target_slot(&meta, options.slot);
    ensure!(
        target != meta.slot,
        "the inactive slot must differ from the confirmed slot"
    );
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

    let root_start = image::ROOT_START_LBA;
    let root_sectors = image::ROOT_SECTORS;
    let second_start = root_start + root_sectors;
    let second_end = second_start + root_sectors - 1;
    ensure!(
        second_end < image::TOTAL_SECTORS,
        "the image has no room for two root slots"
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
            Partition::new(
                TYPE_LINUX_DATA,
                1,
                root_start,
                root_start + root_sectors - 1,
                "ROOT-A",
            ),
            Partition::new(TYPE_LINUX_DATA, 2, second_start, second_end, "ROOT-B"),
        ],
    )?;
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

fn build_fixture(options: &Options, output: &Path) -> Result<()> {
    let kernel = options
        .kernel
        .as_ref()
        .ok_or_else(|| anyhow!("--fixture needs --kernel"))?;
    let kernel = fs::read(kernel).with_context(|| format!("cannot read {}", kernel.display()))?;
    let initfs = match &options.initfs {
        Some(path) => fs::read(path).with_context(|| format!("cannot read {}", path.display()))?,
        None => b"fixture initfs".to_vec(),
    };
    let meta = BootMeta {
        cmdline: String::from(FIXTURE_CMDLINE),
        log_level: String::from("info"),
        slot: Slot::A,
        trial: Some(Slot::B),
        attempts: options.attempts,
        has_slot_config: true,
    };
    let mut builder = FatImageBuilder::new(usize::try_from(image::ESP_SECTORS * SECTOR_SIZE)?)?
        .with_epoch(image::source_date_epoch());
    for dir in ["raven", "raven/slot-a", "raven/slot-b"] {
        builder.add_dir(dir)?;
    }
    builder.add_file("raven/boot.toml", meta.render().as_bytes())?;
    builder.add_file("raven/slot-a/kernel.elf", &kernel)?;
    builder.add_file("raven/slot-a/initfs.img", &initfs)?;
    builder.add_file("raven/slot-b/kernel.elf", b"not an ELF64 image\n")?;
    builder.add_file("raven/slot-b/initfs.img", &initfs)?;
    let esp = builder.finish()?;
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
    if let Some(parent) = output.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("cannot create {}", parent.display()))?;
    }
    image::write_image(output, &gpt, &esp)?;
    println!("{}", output.display());
    Ok(())
}

fn default_image_path(arch: Arch) -> PathBuf {
    PathBuf::from("build")
        .join(arch.as_str())
        .join(format!("ravenbsd-{}.img", arch.as_str()))
}

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
