// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Command-line parsing for the TUI, list, dry-run and helper paths.

use std::path::PathBuf;

use anyhow::{Result, bail};

/// Parsed `raven-flash` options.
#[derive(Clone, Debug)]
pub struct Options {
    /// Print the target list and exit.
    pub list: bool,
    /// Validate the image/target without writing.
    pub dry_run: bool,
    /// Explicit non-interactive write confirmation.
    pub yes: bool,
    /// Raw image path.
    pub image: Option<PathBuf>,
    /// Target block device.
    pub device: Option<PathBuf>,
    /// Run as the privileged write helper.
    pub helper_write: bool,
    /// Typed confirmation token for the helper.
    pub confirm: Option<String>,
    /// Sysfs root (tests use a fixture).
    pub sys_root: PathBuf,
    /// Mount table path (tests use a fixture).
    pub mounts: PathBuf,
    /// Test override for the root disk name.
    pub root_device: Option<String>,
    /// Show usage.
    pub help: bool,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            list: false,
            dry_run: false,
            yes: false,
            image: None,
            device: None,
            helper_write: false,
            confirm: None,
            sys_root: PathBuf::from("/sys"),
            mounts: PathBuf::from("/proc/mounts"),
            root_device: None,
            help: false,
        }
    }
}

/// Usage text.
pub fn usage() -> &'static str {
    "usage:
  raven-flash                      open the TUI
  raven-flash --list               list removable targets (no write)
  raven-flash --dry-run --image IMG [--device DEV]
  raven-flash --yes --image IMG --device DEV
  raven-flash --helper-write --image IMG --device DEV --confirm TOKEN

The TUI is the normal path. --list/--dry-run/--yes are explicit non-TTY paths.
This tool writes a raw image; it never formats and never writes a fixed disk."
}

/// Parse arguments.
pub fn parse(args: &[String]) -> Result<Options> {
    let mut options = Options::default();
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--list" => options.list = true,
            "--dry-run" => options.dry_run = true,
            "--yes" => options.yes = true,
            "--helper-write" => options.helper_write = true,
            "--help" | "-h" => options.help = true,
            "--image" => options.image = Some(value(args, &mut index, "--image")?.into()),
            "--device" => options.device = Some(value(args, &mut index, "--device")?.into()),
            "--confirm" => options.confirm = Some(value(args, &mut index, "--confirm")?),
            "--sys-root" => options.sys_root = value(args, &mut index, "--sys-root")?.into(),
            "--mounts" => options.mounts = value(args, &mut index, "--mounts")?.into(),
            "--root-device" => {
                options.root_device = Some(value(args, &mut index, "--root-device")?)
            }
            other => bail!("unknown option `{other}`\n{}", usage()),
        }
        index += 1;
    }
    if options.helper_write && (options.image.is_none() || options.device.is_none()) {
        bail!("--helper-write needs --image and --device");
    }
    if options.yes && (options.image.is_none() || options.device.is_none()) {
        bail!("--yes needs --image and --device");
    }
    Ok(options)
}

fn value(args: &[String], index: &mut usize, flag: &str) -> Result<String> {
    *index += 1;
    args.get(*index)
        .cloned()
        .ok_or_else(|| anyhow::anyhow!("{flag} needs a value"))
}

#[cfg(test)]
#[path = "cli_ut.rs"]
mod tests;
