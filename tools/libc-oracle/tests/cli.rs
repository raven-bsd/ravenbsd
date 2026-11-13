// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Command-line tests for `libc-oracle` (no oracle run; fixture donor only).

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT: AtomicU64 = AtomicU64::new(0);

struct TempDir {
    path: PathBuf,
}

impl TempDir {
    fn new(label: &str) -> Self {
        let path = std::env::temp_dir().join(format!(
            "libc-oracle-cli-{label}-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&path).expect("temp dir");
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

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

fn real_recipes() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/libc/oracle/recipes")
}

fn copy_tree(from: &Path, to: &Path) {
    fs::create_dir_all(to).expect("destination");
    for entry in fs::read_dir(from).expect("source") {
        let entry = entry.expect("entry");
        let target = to.join(entry.file_name());
        if entry.file_type().expect("type").is_dir() {
            copy_tree(&entry.path(), &target);
        } else {
            fs::copy(entry.path(), target).expect("copy fixture");
        }
    }
}

/// Committed vendor fixture plus an unclassified `.c` written at test time.
fn vendor_resources(temp: &TempDir) -> PathBuf {
    let destination = temp.path().join("resources");
    copy_tree(&fixtures().join("vendor/resources"), &destination);
    fs::write(
        destination.join("fixture/tests/libc/string/t_unknown.c"),
        "/* No licence text in this invented fixture file. */\nint fixture_unknown(void) { return 5; }\n",
    )
    .expect("unknown fixture");
    destination
}

fn run(args: &[&str], env: &[(&str, &str)]) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_libc-oracle"));
    command.args(args);
    for (key, value) in env {
        command.env(key, value);
    }
    command.output().expect("run libc-oracle")
}

#[test]
fn unknown_subcommand_exits_two() {
    let output = run(&["wat"], &[]);
    assert_eq!(output.status.code(), Some(2));
}

#[test]
fn missing_arguments_exit_two() {
    let output = run(&["build"], &[]);
    assert_eq!(output.status.code(), Some(2));
    let output = run(&["adhoc", "openbsd"], &[]);
    assert_eq!(output.status.code(), Some(2));
    let output = run(&["vendor"], &[]);
    assert_eq!(output.status.code(), Some(2));
}

#[test]
fn list_prints_the_real_family() {
    let recipes = real_recipes();
    let output = run(&["list", "--recipes", recipes.to_str().expect("utf8")], &[]);
    assert_eq!(output.status.code(), Some(0));
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.lines().any(|line| line == "string-basic"),
        "{stdout}"
    );
}

#[test]
fn check_with_an_empty_resource_tree_prints_skipped() {
    let temp = TempDir::new("check");
    let recipes = real_recipes();
    let output = run(
        &[
            "check",
            "--recipes",
            recipes.to_str().expect("utf8"),
            "--resources",
            temp.path().to_str().expect("utf8"),
        ],
        &[],
    );
    assert_eq!(output.status.code(), Some(0));
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("skipped"), "{stdout}");
}

#[test]
fn a_missing_cc_exits_77() {
    let fixtures = fixtures();
    let temp = TempDir::new("missing-cc");
    let output = run(
        &[
            "build",
            "fixture",
            "--recipes",
            fixtures.join("recipes").to_str().expect("utf8"),
            "--resources",
            fixtures.to_str().expect("utf8"),
            "--out",
            temp.path().to_str().expect("utf8"),
            "--no-pin-check",
        ],
        &[("CC", "/nonexistent/cc")],
    );
    assert_eq!(output.status.code(), Some(77));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("skipped: cc not found"), "{stderr}");
}

#[test]
fn adhoc_build_prints_the_oracle_directory_last() {
    if !Command::new("cc")
        .arg("--version")
        .status()
        .is_ok_and(|status| status.success())
    {
        println!("SKIP: cc not found");
        return;
    }
    let fixtures = fixtures();
    let temp = TempDir::new("adhoc");
    let output = run(
        &[
            "adhoc",
            "fixture",
            "lib/single.c",
            "--recipes",
            fixtures.join("recipes").to_str().expect("utf8"),
            "--resources",
            fixtures.to_str().expect("utf8"),
            "--out",
            temp.path().to_str().expect("utf8"),
            "--no-pin-check",
        ],
        &[],
    );
    assert_eq!(output.status.code(), Some(0));
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("oracle_fixture_single"), "{stdout}");
    let last = stdout.lines().last().expect("last line");
    assert!(last.starts_with("RAVEN_ORACLE_DIR="), "{last}");
}

#[test]
fn vendor_dry_run_reports_counts_and_writes_nothing() {
    let temp = TempDir::new("vendor-dry");
    let resources = vendor_resources(&temp);
    let output = run(
        &[
            "vendor",
            "--donor",
            "fixture",
            "--from",
            "tests/libc/string",
            "--to",
            temp.path().join("vendor").to_str().expect("utf8"),
            "--resources",
            resources.to_str().expect("utf8"),
            "--no-pin-check",
            "--dry-run",
        ],
        &[],
    );
    assert_eq!(output.status.code(), Some(0));
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("accepted 3, refused 2"), "{stdout}");
    assert!(stdout.contains("class bsd: 1"), "{stdout}");
    assert!(stdout.contains("class isc: 1"), "{stdout}");
    assert!(stdout.contains("class pd: 1"), "{stdout}");
    assert!(stdout.contains("ignored: 1"), "{stdout}");
    assert!(!temp.path().join("vendor").exists());
}

#[test]
fn vendor_writes_review_and_provenance() {
    let temp = TempDir::new("vendor-real");
    let resources = vendor_resources(&temp);
    let to = temp.path().join("vendor");
    let output = run(
        &[
            "vendor",
            "--donor",
            "fixture",
            "--from",
            "tests/libc/string",
            "--to",
            to.to_str().expect("utf8"),
            "--resources",
            resources.to_str().expect("utf8"),
            "--no-pin-check",
            "--date",
            "2026-11-13",
        ],
        &[],
    );
    assert_eq!(output.status.code(), Some(0));
    assert!(to.join("t_bsd.c").is_file());
    assert!(!to.join("t_reject.c").exists());
    let provenance = fs::read_to_string(to.join("provenance.toml")).expect("provenance");
    let parsed: toml::Value = toml::from_str(&provenance).expect("valid toml");
    assert_eq!(parsed["import"].as_array().unwrap().len(), 3);
    let review = fs::read_to_string(to.join("REVIEW.txt")).expect("review");
    assert!(review.contains("t_reject.c"));
    assert!(review.contains("t_unknown.c"));
}
