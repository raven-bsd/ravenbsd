// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Tests for `raven config`.

use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

use raven_output::{Exit, Format};

use super::run_with_root;
use crate::Context;

static NEXT: AtomicUsize = AtomicUsize::new(1);

struct TempDir {
    path: PathBuf,
}

impl TempDir {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "raven-config-cmd-{}-{}",
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

    fn write(&self, relative: &str, text: &str) {
        let path = self.path.join(relative);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, text).unwrap();
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

fn run(root: &Path, args: &[&str], format: Format) -> (Exit, String, String) {
    let args: Vec<OsString> = args.iter().map(OsString::from).collect();
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    let exit = {
        let mut context = Context::new(format, false, &mut stdout, &mut stderr);
        run_with_root(&mut context, root, &args)
    };
    (
        exit,
        String::from_utf8(stdout).unwrap(),
        String::from_utf8(stderr).unwrap(),
    )
}

#[test]
fn check_succeeds_with_defaults() {
    let root = TempDir::new();
    let (exit, stdout, stderr) = run(root.path(), &["check"], Format::Json);
    assert_eq!(exit, Exit::Success);
    assert!(stderr.is_empty());
    let value: serde_json::Value = serde_json::from_str(stdout.trim()).unwrap();
    assert_eq!(value["kind"], "check");
    assert_eq!(value["status"], "ok");
}

#[test]
fn check_reports_an_invalid_file() {
    let root = TempDir::new();
    root.write("etc/raven/profiles/base.toml", "unknown = 1\n");
    let (exit, stdout, stderr) = run(root.path(), &["check"], Format::Json);
    assert_eq!(exit, Exit::Failure);
    assert!(stdout.is_empty());
    let value: serde_json::Value = serde_json::from_str(stderr.trim()).unwrap();
    assert_eq!(value["code"], "ECONFIG");
}

#[test]
fn show_emits_the_effective_json_object() {
    let root = TempDir::new();
    root.write(
        "etc/raven/profiles/base.toml",
        "color = \"never\"\n[log]\nlevel = \"debug\"\n",
    );
    let (exit, stdout, _) = run(root.path(), &["show"], Format::Json);
    assert_eq!(exit, Exit::Success);
    let value: serde_json::Value = serde_json::from_str(stdout.trim()).unwrap();
    assert_eq!(value["kind"], "config");
    assert_eq!(value["config"]["color"], "never");
    assert_eq!(value["config"]["log"]["level"], "debug");
}

#[test]
fn show_emits_toml_in_human_mode() {
    let root = TempDir::new();
    let (exit, stdout, _) = run(root.path(), &["show"], Format::Human);
    assert_eq!(exit, Exit::Success);
    assert!(stdout.contains("profile"));
    assert!(stdout.contains("log"));
}

#[test]
fn get_emits_a_wrapped_value() {
    let root = TempDir::new();
    root.write("etc/raven/profiles/base.toml", "[log]\nlevel = \"warn\"\n");
    let (exit, stdout, _) = run(root.path(), &["get", "log.level"], Format::Json);
    assert_eq!(exit, Exit::Success);
    let value: serde_json::Value = serde_json::from_str(stdout.trim()).unwrap();
    assert_eq!(value["kind"], "config");
    assert_eq!(value["key"], "log.level");
    assert_eq!(value["value"], "warn");
}

#[test]
fn get_missing_key_is_an_error() {
    let root = TempDir::new();
    let (exit, stdout, stderr) = run(root.path(), &["get", "log.nope"], Format::Json);
    assert_eq!(exit, Exit::Failure);
    assert!(stdout.is_empty());
    let value: serde_json::Value = serde_json::from_str(stderr.trim()).unwrap();
    assert_eq!(value["code"], "ENOENT");
}

#[test]
fn unknown_subcommand_is_usage() {
    let root = TempDir::new();
    let (exit, _, _) = run(root.path(), &["wat"], Format::Human);
    assert_eq!(exit, Exit::Usage);
}
