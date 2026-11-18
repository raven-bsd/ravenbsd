// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Test-only scratch directories and tool probes (also usable by integration tests).

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

static COUNTER: AtomicU64 = AtomicU64::new(0);

/// A unique temporary directory removed on drop.
#[derive(Debug)]
pub struct TempDir {
    path: PathBuf,
}

impl TempDir {
    /// Create a directory under `std::env::temp_dir()`.
    #[must_use]
    pub fn new(prefix: &str) -> Self {
        let path = std::env::temp_dir().join(format!(
            "libc-world-{prefix}-{}-{}",
            std::process::id(),
            COUNTER.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(&path).expect("temp dir");
        Self { path }
    }

    /// The directory path.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// A child path.
    #[must_use]
    pub fn join(&self, name: &str) -> PathBuf {
        self.path.join(name)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

/// Write a file, creating parents.
pub fn write(path: &Path, text: &str) {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).expect("parent");
    }
    fs::write(path, text).expect("write");
}

/// Whether a host command exists.
#[must_use]
pub fn tool_available(tool: &str) -> bool {
    Command::new(tool).arg("--version").output().is_ok()
}

/// Whether the required tools for a fixture test exist; prints `SKIP: ...` and returns false when not.
#[must_use]
pub fn require(tools: &[&str]) -> bool {
    for tool in tools {
        if !tool_available(tool) {
            if std::env::var("RL_WORLD_REQUIRED").is_ok_and(|value| value == "1") {
                panic!("missing required tool `{tool}` (RL_WORLD_REQUIRED=1)");
            }
            eprintln!("SKIP: {tool} not found");
            return false;
        }
    }
    true
}

/// Build a `.tar.gz` from a directory with the system `tar` (fixture setup only).
#[must_use]
pub fn make_tar(directory: &Path, name: &str, tarball: &Path) -> bool {
    if !require(&["tar"]) {
        return false;
    }
    let status = Command::new("tar")
        .args(["czf"])
        .arg(tarball)
        .arg("-C")
        .arg(directory)
        .arg(name)
        .status()
        .expect("tar");
    assert!(status.success(), "tar failed");
    true
}

/// Whether `text` names a line starting with `prefix`.
#[must_use]
pub fn has_line(text: &str, prefix: &str) -> bool {
    text.lines().any(|line| line.starts_with(prefix))
}

#[cfg(test)]
#[path = "testutil_ut.rs"]
mod tests;
