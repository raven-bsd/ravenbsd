// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Scripted-input tests for the run loop.

use std::fs;
use std::io::Cursor;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

use super::{Options, Outcome, run};
use crate::model::network_path;

static NEXT: AtomicUsize = AtomicUsize::new(1);

struct TempDir {
    path: PathBuf,
}

impl TempDir {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "raven-tui-run-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(&path).unwrap();
        Self { path }
    }

    fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

fn run_with(root: &Path, options: Options, input: &[u8]) -> (Outcome, String) {
    let mut input = Cursor::new(input.to_vec());
    let mut output = Vec::new();
    let outcome = run(root, options, &mut input, &mut output).unwrap();
    (outcome, String::from_utf8(output).unwrap())
}

#[test]
fn scripted_search_and_edit_save_the_drop_in() {
    let root = TempDir::new();
    let (outcome, output) = run_with(root.path(), Options::default(), b"/net\nraven-tui\n");
    assert!(matches!(outcome, Outcome::Saved(_)));
    assert!(output.contains("raven config: TUI"));
    assert!(output.contains("raven config: module=Network"));
    assert!(output.contains("raven config: saved"));
    let text = fs::read_to_string(network_path(root.path())).unwrap();
    assert!(text.contains("hostname = \"raven-tui\""));
    assert!(text.contains("mode = \"dhcp\""));
}

#[test]
fn scripted_dry_run_outputs_the_plan_without_a_file() {
    let root = TempDir::new();
    let options = Options {
        dry_run: true,
        ..Options::default()
    };
    let (outcome, output) = run_with(root.path(), options, b"/net\ndry\n");
    assert!(matches!(outcome, Outcome::DryRun(_)));
    assert!(output.contains("raven config: dry-run plan="));
    assert!(output.contains("[network]"));
    assert!(!network_path(root.path()).exists());
}

#[test]
fn scripted_cancel_exits_without_a_file() {
    let root = TempDir::new();
    let (outcome, output) = run_with(root.path(), Options::default(), b"q\n");
    assert_eq!(outcome, Outcome::Cancelled);
    assert!(output.contains("raven config: cancelled"));
    assert!(!network_path(root.path()).exists());
}

#[test]
fn incomplete_arrows_wait_for_the_final_byte() {
    assert_eq!(crate::complete_prefix(b"\x1b"), 0);
    assert_eq!(crate::complete_prefix(b"\x1b["), 0);
    assert_eq!(crate::complete_prefix(b"\x1b[B"), 3);
    assert_eq!(crate::complete_prefix(b"\x1b[B\n"), 4);
    assert_eq!(crate::complete_prefix(b"q"), 1);
}
