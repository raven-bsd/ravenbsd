// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Regression test: `xtask docs` is a frozen no-op and names the management tree.

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
fn docs_command_is_a_frozen_no_op() {
    let tree = Tree::new();
    let output = Command::new(xtask())
        .arg("docs")
        .current_dir(&tree.root)
        .output()
        .expect("run xtask docs");
    assert_eq!(output.status.code(), Some(0));
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let combined = format!("{stdout}{stderr}");
    assert!(
        combined.contains("frozen"),
        "frozen docs reason not printed: {combined}"
    );
    assert!(
        combined.contains("ravenbsd-prjmgmt"),
        "management tree not named: {combined}"
    );
}
