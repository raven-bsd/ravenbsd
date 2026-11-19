// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Validated raw write, fsync and readback verification.

use std::fs::{File, OpenOptions};
use std::io::{BufReader, Read, Write};
use std::path::Path;
use std::process::Command;

use anyhow::{Context, Result, bail};
use sha2::{Digest, Sha256};

use crate::device::Device;
use crate::image::ImageInfo;

/// Result of a completed write/verify.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WriteReport {
    /// Image SHA-256.
    pub image_sha256: String,
    /// Target readback SHA-256.
    pub target_sha256: String,
    /// Bytes written.
    pub bytes: u64,
    /// Readback hash matched.
    pub verified: bool,
}

/// Validate a selected image/device pair before any write.
pub fn validate_pair(image: &ImageInfo, device: &Device) -> Result<()> {
    if device.is_root {
        bail!("refusing the root/boot disk {}", device.name);
    }
    if !device.removable {
        bail!("{} is not removable", device.name);
    }
    if !device.holders.is_empty() {
        bail!("{} has holders ({})", device.name, device.holders.join(","));
    }
    if image.size > device.size_bytes {
        bail!(
            "image does not fit: {} > {} bytes",
            image.size,
            device.size_bytes
        );
    }
    if !device.mounted.is_empty() {
        bail!(
            "{} has mounted partitions: {}",
            device.name,
            device
                .mounted
                .iter()
                .map(|mount| mount.target.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        );
    }
    Ok(())
}

/// Unmount every mounted partition of the target.
pub fn unmount(device: &Device) -> Result<()> {
    for mount in &device.mounted {
        let status = Command::new("umount")
            .arg(&mount.target)
            .status()
            .with_context(|| format!("cannot run umount {}", mount.target))?;
        if !status.success() {
            bail!("cannot unmount {} (status {status})", mount.target);
        }
    }
    Ok(())
}

/// Write the image, fsync, then read it back and compare the hash.
pub fn write_and_verify(
    image: &ImageInfo,
    target: &Path,
    verify: bool,
    require_block: bool,
    progress: &mut dyn FnMut(u64, u64),
) -> Result<WriteReport> {
    let metadata = std::fs::metadata(target)
        .with_context(|| format!("cannot stat target {}", target.display()))?;
    if require_block && !is_block_device(&metadata) {
        bail!("target is not a block device: {}", target.display());
    }
    let input = File::open(&image.path)
        .with_context(|| format!("cannot open image {}", image.path.display()))?;
    let mut reader = BufReader::new(input);
    let mut output = OpenOptions::new()
        .write(true)
        .open(target)
        .with_context(|| format!("cannot open target {}", target.display()))?;
    let mut buffer = vec![0u8; 4 * 1024 * 1024];
    let mut written = 0u64;
    loop {
        let read = reader
            .read(&mut buffer)
            .with_context(|| format!("cannot read {}", image.path.display()))?;
        if read == 0 {
            break;
        }
        output
            .write_all(&buffer[..read])
            .with_context(|| format!("cannot write {}", target.display()))?;
        written += read as u64;
        progress(written, image.size);
    }
    output
        .sync_all()
        .with_context(|| format!("cannot fsync {}", target.display()))?;
    drop(output);
    let target_sha256 = if verify {
        hash_prefix(target, image.size)?
    } else {
        String::new()
    };
    let verified = verify && target_sha256 == image.sha256;
    if verify && !verified {
        bail!(
            "readback hash mismatch: image={} target={}",
            image.sha256,
            target_sha256
        );
    }
    Ok(WriteReport {
        image_sha256: image.sha256.clone(),
        target_sha256,
        bytes: written,
        verified,
    })
}

fn is_block_device(metadata: &std::fs::Metadata) -> bool {
    use std::os::unix::fs::FileTypeExt;
    metadata.file_type().is_block_device()
}

fn hash_prefix(path: &Path, limit: u64) -> Result<String> {
    let file = File::open(path).with_context(|| format!("cannot open {}", path.display()))?;
    let mut reader = BufReader::new(file);
    let mut hasher = Sha256::new();
    let mut remaining = limit;
    let mut buffer = vec![0u8; 1024 * 1024];
    while remaining > 0 {
        let want = usize::try_from(remaining.min(buffer.len() as u64)).unwrap_or(buffer.len());
        let read = reader
            .read(&mut buffer[..want])
            .with_context(|| format!("cannot read {}", path.display()))?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
        remaining -= read as u64;
    }
    Ok(format!("{:x}", hasher.finalize()))
}

#[cfg(test)]
#[path = "writer_ut.rs"]
mod tests;
