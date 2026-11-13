// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//!
//! Test fixtures: temporary directories, fake `nm`/`readelf` programs and one real rustc
//! staticlib. A missing rustc or tool prints `SKIP: <reason>`; `RL_LIBC_REQUIRED=1` turns the
//! skip into a failure.

use std::path::{Path, PathBuf};
use std::process::Command;

/// A temporary directory removed on drop.
pub struct TempDir {
    /// The directory.
    pub path: PathBuf,
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

/// Create a fresh temporary directory.
#[must_use]
pub fn tempdir(tag: &str) -> TempDir {
    let path = std::env::temp_dir().join(format!("raven-libc-mkmap-{}-{tag}", std::process::id()));
    let _ = std::fs::remove_dir_all(&path);
    std::fs::create_dir_all(&path).expect("create temp dir");
    TempDir { path }
}

/// Write a file under `dir`.
pub fn write(dir: &Path, name: &str, text: &str) -> PathBuf {
    let path = dir.join(name);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).expect("create parent");
    }
    std::fs::write(&path, text).expect("write fixture");
    path
}

/// Write an executable `/bin/sh` script.
pub fn script(dir: &Path, name: &str, body: &str) -> PathBuf {
    let path = write(dir, name, body);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut permissions = std::fs::metadata(&path).expect("stat script").permissions();
        permissions.set_mode(0o755);
        std::fs::set_permissions(&path, permissions).expect("chmod script");
    }
    path
}

/// Whether `rustc` can be started.
#[must_use]
pub fn rustc_available() -> bool {
    Command::new("rustc")
        .arg("--version")
        .output()
        .is_ok_and(|output| output.status.success())
}

/// Whether `nm` can be started.
#[must_use]
pub fn nm_available() -> bool {
    Command::new("nm")
        .arg("--version")
        .output()
        .is_ok_and(|output| output.status.success())
}

/// A `SKIP:` line, or a panic when the fixture is required.
pub fn skip(reason: &str) {
    if std::env::var("RL_LIBC_REQUIRED").as_deref() == Ok("1") {
        panic!("SKIP: {reason} (RL_LIBC_REQUIRED=1)");
    }
    eprintln!("SKIP: {reason}");
}

/// Build a staticlib from one small source file; `None` when rustc is absent.
pub fn build_staticlib(dir: &Path, crate_name: &str, source: &str) -> Option<PathBuf> {
    if !rustc_available() {
        skip("rustc is missing");
        return None;
    }
    let source_path = write(dir, "fixture.rs", source);
    let archive = dir.join(format!("lib{crate_name}.a"));
    let status = Command::new("rustc")
        .args([
            "--edition",
            "2024",
            "--crate-name",
            crate_name,
            "--crate-type",
            "staticlib",
            "-C",
            "panic=abort",
            "-C",
            "opt-level=z",
            "-C",
            "codegen-units=4096",
        ])
        .arg(&source_path)
        .arg("-o")
        .arg(&archive)
        .status()
        .expect("start rustc");
    if !status.success() {
        skip("rustc failed to build the fixture");
        return None;
    }
    Some(archive)
}
