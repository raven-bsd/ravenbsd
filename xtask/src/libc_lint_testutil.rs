// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Temporary fixture trees for libc-lint tests.

use std::fs;
use std::path::{Path, PathBuf};

pub(crate) struct Tree {
    root: PathBuf,
}

impl Tree {
    pub(crate) fn new() -> Self {
        let root = std::env::temp_dir().join(format!(
            "raven-libc-lint-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|value| value.as_nanos())
                .unwrap_or(0)
        ));
        fs::create_dir_all(&root).expect("temp tree");
        Self { root }
    }

    pub(crate) fn root(&self) -> &Path {
        &self.root
    }

    pub(crate) fn write(&self, path: &str, content: &str) {
        let full = self.root.join(path);
        if let Some(parent) = full.parent() {
            fs::create_dir_all(parent).expect("parent");
        }
        fs::write(full, content).expect("write fixture");
    }

    pub(crate) fn copy_manifest(&self, from: &Path) {
        let target = self.root.join("libc/ravenlibc/xtask");
        fs::create_dir_all(&target).expect("xtask dir");
        for name in [
            "crates.toml",
            "features.toml",
            "unsafe-allowlist.toml",
            "tree.toml",
        ] {
            fs::copy(from.join(name), target.join(name)).expect("copy data");
        }
    }
}

impl Drop for Tree {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}
