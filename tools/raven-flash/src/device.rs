// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Removable-device enumeration and the safety filter.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use std::os::unix::fs::MetadataExt;

use crate::image::ImageInfo;

/// One mounted filesystem.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Mount {
    /// Source device (for example `/dev/sdb1`).
    pub source: String,
    /// Mount point.
    pub target: String,
    /// Filesystem type.
    pub fstype: String,
}

/// One block device and why it may or may not be a safe target.
#[derive(Clone, Debug)]
pub struct Device {
    /// Kernel block name.
    pub name: String,
    /// `/dev/<name>` path.
    pub path: PathBuf,
    /// Model string, when the kernel exposes one.
    pub model: String,
    /// Serial string, when the kernel exposes one.
    pub serial: String,
    /// Size in bytes.
    pub size_bytes: u64,
    /// `usb`, `mmc` or `unknown`.
    pub transport: String,
    /// Kernel removable flag.
    pub removable: bool,
    /// Mounted partitions belonging to this disk.
    pub mounted: Vec<Mount>,
    /// sysfs holders (LVM/md members).
    pub holders: Vec<String>,
    /// This disk backs `/` or `/boot`.
    pub is_root: bool,
}

impl Device {
    /// One display line.
    pub fn summary(&self) -> String {
        let mounts = if self.mounted.is_empty() {
            "not mounted".to_string()
        } else {
            format!(
                "mounted: {}",
                self.mounted
                    .iter()
                    .map(|mount| mount.target.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        };
        format!(
            "{} {} model={} serial={} size={} transport={} {}",
            self.name,
            self.path.display(),
            if self.model.is_empty() {
                "unknown"
            } else {
                &self.model
            },
            if self.serial.is_empty() {
                "unknown"
            } else {
                &self.serial
            },
            self.size_bytes,
            self.transport,
            mounts
        )
    }
}

/// A device plus the safety-filter verdict.
#[derive(Clone, Debug)]
pub struct Candidate {
    /// Device details.
    pub device: Device,
    /// `None` for a safe target, otherwise the refusal reason.
    pub reject: Option<String>,
}

/// Where to read sysfs, mounts and device nodes (fixtures override these).
#[derive(Clone, Debug)]
pub struct FilterConfig {
    /// Sysfs root.
    pub sys_root: PathBuf,
    /// Mount table.
    pub mounts: PathBuf,
    /// `/dev` root.
    pub dev_root: PathBuf,
    /// Explicit root disk name (tests).
    pub root_device: Option<String>,
}

impl Default for FilterConfig {
    fn default() -> Self {
        Self {
            sys_root: PathBuf::from("/sys"),
            mounts: PathBuf::from("/proc/mounts"),
            dev_root: PathBuf::from("/dev"),
            root_device: None,
        }
    }
}

/// Enumerate every block device and apply the filter.
pub fn enumerate(config: &FilterConfig, image: Option<&ImageInfo>) -> Result<Vec<Candidate>> {
    let block = config.sys_root.join("block");
    let entries = match std::fs::read_dir(&block) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => {
            return Err(error).with_context(|| format!("cannot read {}", block.display()));
        }
    };
    let mounts = parse_mounts(&std::fs::read_to_string(&config.mounts).unwrap_or_default());
    let root = config
        .root_device
        .clone()
        .or_else(|| root_disk(&config.sys_root));
    let mut devices = Vec::new();
    for entry in entries {
        let entry = entry?;
        let name = entry.file_name().to_string_lossy().to_string();
        devices.push(inspect_device(
            config,
            &name,
            &mounts,
            root.as_deref(),
            image,
        ));
    }
    devices.sort_by(|left, right| left.device.name.cmp(&right.device.name));
    Ok(devices)
}

fn inspect_device(
    config: &FilterConfig,
    name: &str,
    mounts: &[Mount],
    root: Option<&str>,
    image: Option<&ImageInfo>,
) -> Candidate {
    let sys = config.sys_root.join("block").join(name);
    let read = |relative: &str| {
        std::fs::read_to_string(sys.join(relative))
            .map(|value| value.trim().to_string())
            .unwrap_or_default()
    };
    let size_sectors = read("size").parse::<u64>().unwrap_or(0);
    let size_bytes = size_sectors.saturating_mul(512);
    let removable = read("removable") == "1";
    let transport = transport(&sys);
    let holders = list_dir(&sys.join("holders"));
    let mounted: Vec<Mount> = mounts
        .iter()
        .filter(|mount| mount_belongs(&mount.source, name))
        .cloned()
        .collect();
    let is_root = root == Some(name);
    let device = Device {
        name: name.to_string(),
        path: config.dev_root.join(name),
        model: read("device/model"),
        serial: read("device/serial"),
        size_bytes,
        transport,
        removable,
        mounted,
        holders,
        is_root,
    };
    let reject = reject_reason(&device, image);
    Candidate { device, reject }
}

fn reject_reason(device: &Device, image: Option<&ImageInfo>) -> Option<String> {
    let name = device.name.as_str();
    if name.starts_with("loop")
        || name.starts_with("ram")
        || name.starts_with("zram")
        || name.starts_with("dm-")
        || name.starts_with("md")
        || name.starts_with("nbd")
        || name.starts_with("sr")
    {
        return Some("loop/LVM/md/virtual device".to_string());
    }
    if device.is_root {
        return Some("root/boot disk".to_string());
    }
    if !device.holders.is_empty() {
        return Some(format!("has holders: {}", device.holders.join(",")));
    }
    if !device.removable {
        return Some("not removable".to_string());
    }
    if device.transport == "unknown" {
        return Some("transport is not USB/MMC/SD".to_string());
    }
    if let Ok(metadata) = std::fs::symlink_metadata(&device.path)
        && metadata.file_type().is_symlink()
    {
        return Some("target path is a symlink".to_string());
    }
    if let Some(image) = image {
        if image.size > device.size_bytes {
            return Some(format!(
                "image does not fit ({} > {})",
                image.size, device.size_bytes
            ));
        }
        let canonical_image =
            std::fs::canonicalize(&image.path).unwrap_or_else(|_| image.path.clone());
        for mount in &device.mounted {
            let target = Path::new(&mount.target);
            let canonical_target = std::fs::canonicalize(target).unwrap_or_else(|_| target.into());
            if canonical_image.starts_with(&canonical_target) {
                return Some("image is on the target device".to_string());
            }
        }
        if let Ok(meta) = std::fs::metadata(&device.path)
            && meta.rdev() != 0
            && std::fs::metadata(&image.path)
                .map(|image_meta| image_meta.dev())
                .ok()
                == Some(meta.rdev())
        {
            return Some("image is on the target device".to_string());
        }
    }
    None
}

fn transport(sys: &Path) -> String {
    let canonical = std::fs::canonicalize(sys).unwrap_or_else(|_| sys.to_path_buf());
    let text = canonical.to_string_lossy();
    if text.contains("/usb") {
        return "usb".to_string();
    }
    if text.contains("/mmc") {
        return "mmc".to_string();
    }
    let uevent = std::fs::read_to_string(sys.join("device/uevent")).unwrap_or_default();
    if uevent.contains("usb") {
        "usb".to_string()
    } else if uevent.contains("mmc") {
        "mmc".to_string()
    } else {
        "unknown".to_string()
    }
}

fn list_dir(path: &Path) -> Vec<String> {
    std::fs::read_dir(path)
        .map(|entries| {
            entries
                .filter_map(|entry| entry.ok())
                .map(|entry| entry.file_name().to_string_lossy().to_string())
                .collect()
        })
        .unwrap_or_default()
}

/// Parse `/proc/mounts`-shaped text.
pub fn parse_mounts(text: &str) -> Vec<Mount> {
    text.lines()
        .filter_map(|line| {
            let mut fields = line.split_whitespace();
            let source = fields.next()?;
            let target = fields.next()?;
            let fstype = fields.next()?;
            if !source.starts_with("/dev/") {
                return None;
            }
            Some(Mount {
                source: source.to_string(),
                target: target.to_string(),
                fstype: fstype.to_string(),
            })
        })
        .collect()
}

fn mount_belongs(source: &str, name: &str) -> bool {
    let base = format!("/dev/{name}");
    if source == base {
        return true;
    }
    let Some(rest) = source.strip_prefix(&base) else {
        return false;
    };
    rest.starts_with(|value: char| value.is_ascii_digit() || value == 'p')
}

fn root_disk(sys_root: &Path) -> Option<String> {
    for target in ["/", "/boot"] {
        let Ok(metadata) = std::fs::metadata(target) else {
            continue;
        };
        let dev = metadata.dev();
        let major = (dev >> 8) & 0xfff;
        let minor = dev & 0xff;
        let link = sys_root.join(format!("dev/block/{major}:{minor}"));
        let Ok(real) = std::fs::canonicalize(link) else {
            continue;
        };
        let mut after_block = false;
        for component in real.components() {
            let text = component.as_os_str().to_string_lossy();
            if after_block {
                return Some(text.to_string());
            }
            if text == "block" {
                after_block = true;
            }
        }
    }
    None
}

#[cfg(test)]
#[path = "device_ut.rs"]
mod tests;
