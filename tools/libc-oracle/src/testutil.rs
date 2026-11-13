// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Test-only temporary directories and file writers.
//!
//! No external crate is used: the path is `std::env::temp_dir()` plus the process id and a
//! process-local counter, and the directory is removed on drop.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

static NEXT: AtomicU64 = AtomicU64::new(0);

/// A unique temporary directory removed on drop.
pub(crate) struct TempDir {
    path: PathBuf,
}

impl TempDir {
    /// Create a directory named after `label`.
    pub(crate) fn new(label: &str) -> Self {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |duration| duration.subsec_nanos());
        let counter = NEXT.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "libc-oracle-{label}-{}-{counter}-{nanos}",
            std::process::id()
        ));
        fs::create_dir_all(&path).expect("create temporary directory");
        Self { path }
    }

    /// The directory path.
    pub(crate) fn path(&self) -> &Path {
        &self.path
    }

    /// Write `text` to `relative`, creating parent directories.
    pub(crate) fn write(&self, relative: &str, text: &str) -> PathBuf {
        let path = self.path.join(relative);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("create fixture parent");
        }
        fs::write(&path, text).expect("write fixture");
        path
    }

    /// Create a directory under the root.
    pub(crate) fn directory(&self, relative: &str) -> PathBuf {
        let path = self.path.join(relative);
        fs::create_dir_all(&path).expect("create fixture directory");
        path
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

#[cfg(test)]
#[path = "testutil_ut.rs"]
mod tests;
