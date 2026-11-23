// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Addon-binary freshness for `cargo xtask image`.
//!
//! The release image stages prebuilt addon binaries such as `build/ssh/x86_64/bin/sshd`. The
//! profile's `generated = true` entries are not rebuilt by the image step, so a binary linked
//! against an older libc archive or an older addon source can be packed silently. This module
//! refuses the image when a staged addon binary is older than its addon sources, the Raven libc
//! archive/crt objects, or the addon's own staged sysroot.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use anyhow::{Result, bail};

use crate::build::Arch;
use crate::profiles::Profile;

/// The addon name in a staged source such as `build/ssh/x86_64/bin/sshd`, if any.
#[must_use]
pub(crate) fn addon_of(source: &str) -> Option<&str> {
    let rest = source.strip_prefix("build/")?;
    let (addon, _) = rest.split_once('/')?;
    if addon.is_empty() || addon == "cargo" || addon == "fixtures" || addon == "qtest" {
        None
    } else {
        Some(addon)
    }
}

/// The newest modification time among the paths that exist.
fn newest(inputs: &[PathBuf]) -> Option<(SystemTime, PathBuf)> {
    let mut newest: Option<(SystemTime, PathBuf)> = None;
    for path in inputs {
        let Ok(metadata) = fs::metadata(path) else {
            continue;
        };
        let Ok(modified) = metadata.modified() else {
            continue;
        };
        if newest.as_ref().is_none_or(|(time, _)| modified > *time) {
            newest = Some((modified, path.clone()));
        }
    }
    newest
}

/// Whether `binary` is older than any input, returning the first newer input.
#[must_use]
pub(crate) fn newer_input(binary: &Path, inputs: &[PathBuf]) -> Option<PathBuf> {
    let Ok(metadata) = fs::metadata(binary) else {
        return None;
    };
    let Ok(built) = metadata.modified() else {
        return None;
    };
    newest(inputs)
        .filter(|(modified, _)| *modified > built)
        .map(|(_, path)| path)
}

/// Whether `binary` is older than any input.
#[cfg(test)]
#[must_use]
pub(crate) fn stale(binary: &Path, inputs: &[PathBuf]) -> bool {
    newer_input(binary, inputs).is_some()
}

/// Every file under `dir`, recursively.
fn files_under(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            files_under(&path, out);
        } else {
            out.push(path);
        }
    }
}

/// The addon source tree, the Raven libc archive/crt objects and the addon's staged sysroot.
#[must_use]
pub(crate) fn addon_inputs(root: &Path, addon: &str, arch: &str) -> Vec<PathBuf> {
    let mut inputs = Vec::new();
    files_under(&root.join("addons").join(addon), &mut inputs);
    let stage = root.join("build").join(arch);
    for name in ["libc.a", "crt1.o", "crti.o", "crtn.o", "Scrt1.o", "rcrt1.o"] {
        inputs.push(stage.join(name));
    }
    inputs.push(
        root.join("build")
            .join(addon)
            .join(arch)
            .join("sysroot/usr/lib/libc.a"),
    );
    inputs
}

/// Refuse an image whose staged addon binaries are older than their inputs.
pub(crate) fn ensure_current(root: &Path, arch: Arch, profile: &Profile) -> Result<()> {
    let arch_name = arch.as_str();
    for file in &profile.initfs.files {
        if !file.generated {
            continue;
        }
        let source = file.src.replace("{arch}", arch_name);
        let Some(addon) = addon_of(&source) else {
            continue;
        };
        if !root.join("addons").join(addon).is_dir() {
            continue;
        }
        let binary = root.join(&source);
        if !binary.is_file() {
            continue;
        }
        let inputs = addon_inputs(root, addon, arch_name);
        if let Some(newer) = newer_input(&binary, &inputs) {
            bail!(
                "stale addon binary {}: {} is newer; rebuild with `sh addons/{}/build.sh`",
                binary.display(),
                newer.display(),
                addon
            );
        }
        let sysroot = root
            .join("build")
            .join(addon)
            .join(arch_name)
            .join("sysroot/usr/lib/libc.a");
        let libc = root.join("build").join(arch_name).join("libc.a");
        if let (Ok(sysroot_meta), Ok(libc_meta)) = (fs::metadata(&sysroot), fs::metadata(&libc))
            && let (Ok(sysroot_time), Ok(libc_time)) =
                (sysroot_meta.modified(), libc_meta.modified())
            && libc_time > sysroot_time
        {
            bail!(
                "stale addon sysroot {}: {} is newer; rebuild with `sh addons/{}/build.sh`",
                sysroot.display(),
                libc.display(),
                addon
            );
        }
    }
    Ok(())
}

#[cfg(test)]
#[path = "addons_ut.rs"]
mod tests;
