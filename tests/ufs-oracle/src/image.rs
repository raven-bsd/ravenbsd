// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Image loading, workspace paths and the in-memory block device.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use ufs_core::{BlockIo, IoError};

/// A RAM-backed block device used by the differential tests.
pub struct VecDisk {
    pub data: Vec<u8>,
}

impl VecDisk {
    pub fn new(data: Vec<u8>) -> Self {
        Self { data }
    }
}

impl BlockIo for VecDisk {
    fn size_bytes(&self) -> u64 {
        self.data.len() as u64
    }

    fn read_at(&mut self, offset: u64, buf: &mut [u8]) -> Result<(), IoError> {
        let start = usize::try_from(offset).map_err(|_| IoError)?;
        let end = start.checked_add(buf.len()).ok_or(IoError)?;
        let src = self.data.get(start..end).ok_or(IoError)?;
        buf.copy_from_slice(src);
        Ok(())
    }

    fn write_at(&mut self, offset: u64, buf: &[u8]) -> Result<(), IoError> {
        let start = usize::try_from(offset).map_err(|_| IoError)?;
        let end = start.checked_add(buf.len()).ok_or(IoError)?;
        let dst = self.data.get_mut(start..end).ok_or(IoError)?;
        dst.copy_from_slice(buf);
        Ok(())
    }

    fn flush(&mut self) -> Result<(), IoError> {
        Ok(())
    }
}

/// Repository root, derived from this crate's compile-time manifest path.
pub fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap_or_else(|_| PathBuf::from(env!("CARGO_MANIFEST_DIR")))
}

/// Vector directory override (`UFS_VECTORS_DIR`) or `tests/ufs-vectors`.
pub fn vectors_dir() -> PathBuf {
    std::env::var_os("UFS_VECTORS_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| workspace_root().join("tests/ufs-vectors"))
}

/// Load `golden/<name>.img.gz` (or `UFS_ORACLE_IMAGE` when set).
pub fn load_golden(name: &str) -> Result<Vec<u8>, String> {
    if let Some(raw) = std::env::var_os("UFS_ORACLE_IMAGE") {
        return fs::read(&raw)
            .map_err(|error| format!("read UFS_ORACLE_IMAGE {}: {error}", raw.display()));
    }
    let path = vectors_dir().join("golden").join(format!("{name}.img.gz"));
    let output = Command::new("gzip")
        .arg("-dc")
        .arg(&path)
        .output()
        .map_err(|error| format!("run gzip for {}: {error}", path.display()))?;
    if !output.status.success() {
        return Err(format!(
            "gzip failed for {}: {}",
            path.display(),
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    Ok(output.stdout)
}

/// Create and return a build directory below the ignored `build/` tree.
pub fn build_dir(name: &str) -> Result<PathBuf, String> {
    let path = workspace_root().join("build").join(name);
    fs::create_dir_all(&path).map_err(|error| format!("create {}: {error}", path.display()))?;
    Ok(path)
}
