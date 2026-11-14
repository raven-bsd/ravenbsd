// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Scratch directories and fixture helpers for the unit tests.
//!
//! The directory names are deterministic per process and counter; a test owns its directory
//! and removes it on drop. No shell, no `tempfile` crate.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

static COUNTER: AtomicU64 = AtomicU64::new(0);

/// One owned scratch directory.
pub struct TempDir {
    path: PathBuf,
}

impl TempDir {
    /// Create a directory under the system temporary directory.
    ///
    /// # Errors
    /// An I/O error.
    pub fn new(label: &str) -> std::io::Result<Self> {
        let counter = COUNTER.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "libc-size-{label}-{}-{counter}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path)?;
        Ok(Self { path })
    }

    /// The directory path.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Write a relative file, creating parents.
    ///
    /// # Errors
    /// An I/O error.
    pub fn write(&self, relative: &str, contents: &[u8]) -> std::io::Result<PathBuf> {
        let path = self.path.join(relative);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&path, contents)?;
        Ok(path)
    }

    /// Read a relative file.
    ///
    /// # Errors
    /// An I/O error.
    pub fn read(&self, relative: &str) -> std::io::Result<String> {
        std::fs::read_to_string(self.path.join(relative))
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

/// A fake `llvm-size` Berkeley output for one executable.
#[must_use]
pub fn size_output(text: u64, data: u64, bss: u64) -> String {
    format!(
        "   text\t   data\t    bss\t    dec\t    hex\tfilename\n\
         \t{text}\t      {data}\t      {bss}\t     0\t     0\tsample\n"
    )
}

/// A fake `llvm-size` output with an explicit file name.
#[must_use]
pub fn size_output_named(name: &str, text: u64, data: u64, bss: u64) -> String {
    format!(
        "   text\t   data\t    bss\t    dec\t    hex\tfilename\n\t{text}\t      {data}\t      {bss}\t     0\t     0\t{name}\n"
    )
}
