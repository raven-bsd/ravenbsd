// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Raw image inspection: size, SHA-256 and optional build metadata.

use std::fs::File;
use std::io::{BufReader, Read};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use sha2::{Digest, Sha256};

/// One inspected raw image.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ImageInfo {
    /// Absolute or user-supplied path.
    pub path: PathBuf,
    /// File size in bytes.
    pub size: u64,
    /// SHA-256 of the complete file.
    pub sha256: String,
    /// Optional build id / commit from an adjacent manifest.
    pub build_id: Option<String>,
}

impl ImageInfo {
    /// A short display line for the TUI and dry-run output.
    pub fn summary(&self) -> String {
        let id = self
            .build_id
            .as_deref()
            .map(|value| format!(" build={value}"))
            .unwrap_or_default();
        format!(
            "{} ({} bytes, sha256 {}{})",
            self.path.display(),
            self.size,
            self.sha256,
            id
        )
    }
}

/// Inspect a raw `.img` file. Refuses directories, empty files and non-`.img` names.
pub fn inspect(path: &Path) -> Result<ImageInfo> {
    let metadata =
        std::fs::metadata(path).with_context(|| format!("cannot read image {}", path.display()))?;
    if !metadata.is_file() {
        bail!("image is not a regular file: {}", path.display());
    }
    if metadata.len() == 0 {
        bail!("image is empty: {}", path.display());
    }
    let extension = path.extension().and_then(|value| value.to_str());
    if extension != Some("img") {
        bail!("image must be a raw .img file: {}", path.display());
    }
    let sha256 = sha256_file(path)?;
    Ok(ImageInfo {
        path: path.to_path_buf(),
        size: metadata.len(),
        sha256,
        build_id: build_id(path),
    })
}

/// Stream a file through SHA-256.
pub fn sha256_file(path: &Path) -> Result<String> {
    let file = File::open(path).with_context(|| format!("cannot open {}", path.display()))?;
    let mut reader = BufReader::new(file);
    let mut hasher = Sha256::new();
    let mut buffer = vec![0u8; 1024 * 1024];
    loop {
        let read = reader
            .read(&mut buffer)
            .with_context(|| format!("cannot read {}", path.display()))?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

fn build_id(image: &Path) -> Option<String> {
    let parent = image.parent()?;
    for name in ["manifest.txt", "build-id.txt"] {
        let path = parent.join(name);
        let Ok(text) = std::fs::read_to_string(&path) else {
            continue;
        };
        for line in text.lines() {
            for prefix in ["commit=", "build_id=", "build-id="] {
                if let Some(value) = line.strip_prefix(prefix) {
                    let value = value.trim();
                    if !value.is_empty() {
                        return Some(value.to_string());
                    }
                }
            }
        }
    }
    None
}

#[cfg(test)]
#[path = "image_ut.rs"]
mod tests;
