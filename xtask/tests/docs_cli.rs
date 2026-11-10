// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Regression test: `xtask docs` must fail and name a missing page in a fixture tree.

use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};

static NEXT_TREE: AtomicUsize = AtomicUsize::new(0);

struct Tree {
    root: PathBuf,
}

impl Tree {
    fn new() -> Self {
        let id = NEXT_TREE.fetch_add(1, Ordering::Relaxed);
        let root = std::env::temp_dir().join(format!("raven-docs-cli-{}-{id}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).expect("create fixture root");
        let status = Command::new("git")
            .args(["init", "-q"])
            .current_dir(&root)
            .status()
            .expect("run git init");
        assert!(status.success(), "git init failed");
        Self { root }
    }

    fn write(&self, relative: &str, contents: &str) {
        let path = self.root.join(relative);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("create fixture parent");
        }
        fs::write(path, contents).expect("write fixture file");
    }

    fn missing_docs_fixture(&self) {
        self.write(
            "crates/raven-abi/src/syscall.rs",
            "syscalls! {\n    EXIT = 0x01;\n}\n",
        );
        self.write(
            "userland/init/Cargo.toml",
            "[package]\nname = \"raven-init\"\n",
        );
    }
}

impl Drop for Tree {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn xtask() -> &'static str {
    env!("CARGO_BIN_EXE_xtask")
}

#[test]
fn docs_command_reports_a_missing_page() {
    let tree = Tree::new();
    tree.missing_docs_fixture();
    let output = Command::new(xtask())
        .arg("docs")
        .current_dir(&tree.root)
        .output()
        .expect("run xtask docs");
    assert_eq!(output.status.code(), Some(1));
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let combined = format!("{stdout}{stderr}");
    assert!(
        combined.contains("docs/man2/exit.2"),
        "missing syscall page not listed: {combined}"
    );
    assert!(
        combined.contains("docs/man8/init.8"),
        "missing init page not listed: {combined}"
    );
}

#[test]
fn docs_command_passes_when_pages_are_present() {
    let tree = Tree::new();
    tree.missing_docs_fixture();
    tree.write(
        "docs/man2/exit.2",
        ".Dd January 1, 2026\n.Dt EXIT 2\n.Os RavenBSD\n.Sh NAME\n.Nm exit\n.Nd exit\n.Sh SYNOPSIS\n.Nm\n.Sh DESCRIPTION\nTest page.\n",
    );
    tree.write(
        "docs/man8/init.8",
        ".Dd January 1, 2026\n.Dt INIT 8\n.Os RavenBSD\n.Sh NAME\n.Nm init\n.Nd init\n.Sh SYNOPSIS\n.Nm\n.Sh DESCRIPTION\nTest page.\n",
    );
    let output = Command::new(xtask())
        .arg("docs")
        .current_dir(&tree.root)
        .output()
        .expect("run xtask docs");
    assert_eq!(
        output.status.code(),
        Some(0),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}
