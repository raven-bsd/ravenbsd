// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! End-to-end tests of the `libc-census` binary.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};

const BIN: &str = env!("CARGO_BIN_EXE_libc-census");
static COUNTER: AtomicU64 = AtomicU64::new(0);

struct TempDir {
    path: PathBuf,
}

impl TempDir {
    fn new(label: &str) -> Self {
        let mut path = std::env::temp_dir();
        path.push(format!(
            "libc-census-cli-{label}-{}-{}",
            std::process::id(),
            COUNTER.fetch_add(1, Ordering::SeqCst)
        ));
        fs::create_dir_all(&path).expect("create temp dir");
        Self { path }
    }

    fn path(&self) -> &Path {
        &self.path
    }

    fn read(&self, relative: &str) -> String {
        fs::read_to_string(self.path.join(relative)).expect("read test file")
    }

    fn write(&self, relative: &str, content: &str) {
        let path = self.path.join(relative);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("create parent");
        }
        fs::write(path, content).expect("write test file");
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name)
}

fn command(temp: &TempDir, args: &[&str]) -> Output {
    Command::new(BIN)
        .args(args)
        .current_dir(temp.path())
        .output()
        .expect("run libc-census")
}

fn fixture_run(temp: &TempDir, out: &str) -> Output {
    command(
        temp,
        &[
            "run",
            "--resources",
            fixture("resources").to_str().unwrap(),
            "--aports",
            fixture("aports").to_str().unwrap(),
            "--sources",
            fixture("sources").to_str().unwrap(),
            "--out",
            out,
        ],
    )
}

#[test]
fn run_fixture_writes_all_outputs() {
    let temp = TempDir::new("run");
    let result = fixture_run(&temp, "out");
    assert_eq!(
        result.status.code(),
        Some(0),
        "stderr: {}",
        String::from_utf8_lossy(&result.stderr)
    );
    for name in [
        "cells.json",
        "symlists.json",
        "standards.json",
        "ports.json",
        "uses.json",
        "run-summary.json",
    ] {
        assert!(temp.path().join("out").join(name).is_file(), "{name}");
    }
    let summary = temp.read("out/run-summary.json");
    assert!(summary.contains("\"complete\": true"), "{summary}");
}

#[test]
fn merge_fixture_writes_the_census_files() {
    let temp = TempDir::new("merge");
    assert_eq!(fixture_run(&temp, "out").status.code(), Some(0));
    let result = command(&temp, &["merge", "--out", "out", "--census", "census"]);
    assert_eq!(
        result.status.code(),
        Some(0),
        "stderr: {}",
        String::from_utf8_lossy(&result.stderr)
    );
    let symbols = temp.read("census/symbols.toml");
    assert!(symbols.starts_with("# SPDX-License-Identifier: BSD-3-Clause\n"));
    assert!(symbols.contains("[[symbol]]"));
    for name in [
        "census.json",
        "census.md",
        "ring_diff.md",
        "patchset-gaps.md",
        "known-differences.toml",
    ] {
        assert!(temp.path().join("census").join(name).is_file(), "{name}");
    }
}

#[test]
fn the_fixture_table_compare_passes() {
    let temp = TempDir::new("compare");
    let result = command(
        &temp,
        &[
            "compare-table",
            "--table",
            fixture("table15-excerpt.md").to_str().unwrap(),
            "--resources",
            fixture("resources").to_str().unwrap(),
        ],
    );
    assert_eq!(
        result.status.code(),
        Some(0),
        "stderr: {}",
        String::from_utf8_lossy(&result.stderr)
    );
    let stdout = String::from_utf8_lossy(&result.stdout);
    assert!(stdout.contains("result: PASS"), "{stdout}");
    assert!(
        temp.path()
            .join("tools/libc-census/out/compare-table.md")
            .is_file()
    );
}

#[test]
fn a_flipped_table_cell_exits_one() {
    let temp = TempDir::new("compare-flip");
    let table = temp.path().join("table.md");
    fs::write(
        &table,
        "#### 3.B.4\n\
         | Function | glibc | musl | FBSD | NBSD | OBSD | bionic | illumos | mlibc | picolibc | n6 | Ring | Action |\n\
         |---|---|---|---|---|---|---|---|---|---|---|---|---|\n\
         | `strlcpy` | 2/2 | 1/1 | 1/1 | 1/1 | 1/1 | 0/1 | 1/1 | 1/1 | 1/1 | 6 | 1 | x |\n\
         #### 3.B.5\n",
    )
    .unwrap();
    let result = command(
        &temp,
        &[
            "compare-table",
            "--table",
            table.to_str().unwrap(),
            "--resources",
            fixture("resources").to_str().unwrap(),
        ],
    );
    assert_eq!(result.status.code(), Some(1));
    let stdout = String::from_utf8_lossy(&result.stdout);
    assert!(stdout.contains("strlcpy"), "{stdout}");
    assert!(stdout.contains("result: FAIL"), "{stdout}");
}

#[test]
fn require_all_with_a_missing_donor_exits_two() {
    let temp = TempDir::new("compare-partial");
    temp.write(
        "resources/glibc/include/string.h",
        "extern int strlcpy(void);\n",
    );
    let result = command(
        &temp,
        &[
            "compare-table",
            "--table",
            fixture("table15-excerpt.md").to_str().unwrap(),
            "--resources",
            "resources",
            "--require-all",
        ],
    );
    assert_eq!(result.status.code(), Some(2));
    let stdout = String::from_utf8_lossy(&result.stdout);
    assert!(stdout.contains("partial"), "{stdout}");
}

#[test]
fn usage_errors_exit_two_with_the_usage_text() {
    let temp = TempDir::new("usage");
    let result = command(&temp, &[]);
    assert_eq!(result.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&result.stderr).contains("usage:"));
    let result = command(&temp, &["run", "--bogus"]);
    assert_eq!(result.status.code(), Some(2));
    let result = command(&temp, &["merge", "--out"]);
    assert_eq!(result.status.code(), Some(2));
    let result = command(&temp, &["missing"]);
    assert_eq!(result.status.code(), Some(2));
    assert!(
        String::from_utf8_lossy(&result.stderr).contains("--implemented is required"),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
}

#[test]
fn seed_assignments_writes_sorted_rows() {
    let temp = TempDir::new("seed");
    let result = command(
        &temp,
        &[
            "seed-assignments",
            "--api-surface",
            fixture("api-surface-excerpt.md").to_str().unwrap(),
            "--out",
            "assignments.toml",
        ],
    );
    assert_eq!(result.status.code(), Some(0));
    let text = temp.read("assignments.toml");
    assert!(text.starts_with("# SPDX-License-Identifier: BSD-3-Clause\n"));
    assert!(text.contains("[[assign]]"));
    let strlcat = text.find("name = \"strlcat\"").unwrap();
    let strlcpy = text.find("name = \"strlcpy\"").unwrap();
    assert!(strlcat < strlcpy);
}

#[test]
fn an_empty_resources_directory_still_exits_zero() {
    let temp = TempDir::new("empty");
    fs::create_dir_all(temp.path().join("resources")).unwrap();
    let result = command(&temp, &["run", "--resources", "resources", "--out", "out"]);
    assert_eq!(
        result.status.code(),
        Some(0),
        "stderr: {}",
        String::from_utf8_lossy(&result.stderr)
    );
    let cells = temp.read("out/cells.json");
    assert_eq!(cells.trim(), "[]");
    assert_eq!(
        command(&temp, &["merge", "--out", "out", "--census", "census"])
            .status
            .code(),
        Some(0)
    );
    let symbols = temp.read("census/symbols.toml");
    assert!(!symbols.contains("[[symbol]]"));
}

#[test]
fn three_present_donors_make_the_run_partial() {
    let temp = TempDir::new("partial");
    temp.write(
        "resources/glibc/string.c",
        "int strlcpy(void) { return 0; }\n",
    );
    temp.write(
        "resources/musl/src/string.c",
        "int strlcpy(void) { return 0; }\n",
    );
    temp.write(
        "resources/bionic/libc/string.c",
        "int strlcpy(void) { return 0; }\n",
    );
    temp.write("names.txt", "strlcpy\n");
    let result = command(
        &temp,
        &[
            "run",
            "--resources",
            "resources",
            "--names",
            "names.txt",
            "--out",
            "out",
        ],
    );
    assert_eq!(result.status.code(), Some(0));
    let summary = temp.read("out/run-summary.json");
    assert!(summary.contains("\"complete\": false"), "{summary}");
    let cells = temp.read("out/cells.json");
    assert!(
        cells.contains("\"n6\": 3") || cells.contains("\"n6\": 2"),
        "{cells}"
    );
}

#[test]
fn jobs_one_and_four_produce_identical_files() {
    let temp = TempDir::new("jobs");
    assert_eq!(fixture_run(&temp, "out1").status.code(), Some(0));
    let result = command(
        &temp,
        &[
            "run",
            "--resources",
            fixture("resources").to_str().unwrap(),
            "--aports",
            fixture("aports").to_str().unwrap(),
            "--sources",
            fixture("sources").to_str().unwrap(),
            "--out",
            "out4",
            "--jobs",
            "4",
        ],
    );
    assert_eq!(result.status.code(), Some(0));
    for name in [
        "cells.json",
        "symlists.json",
        "standards.json",
        "ports.json",
        "uses.json",
        "run-summary.json",
    ] {
        assert_eq!(
            temp.read(&format!("out1/{name}")),
            temp.read(&format!("out4/{name}")),
            "{name}"
        );
    }
}

#[test]
fn missing_fail_p0_exits_one() {
    let temp = TempDir::new("missing");
    temp.write(
        "out/symbols.toml",
        "schema = 1\n[[symbol]]\nname = \"strlcpy\"\nring = 1\nring_computed = 1\nring_source = \"computed\"\n",
    );
    temp.write(
        "out/ports.json",
        "{\"snapshots\":{\"aports\":{\"packages\":1}},\"packages\":{},\"symbols\":{\"strlcpy\":3},\"headers\":{},\"macros\":{}}",
    );
    temp.write("implemented.txt", "");
    let result = command(
        &temp,
        &[
            "missing",
            "--implemented",
            "implemented.txt",
            "--out",
            "out",
            "--fail-p0",
        ],
    );
    assert_eq!(result.status.code(), Some(1));
    let stdout = String::from_utf8_lossy(&result.stdout);
    assert!(stdout.contains("P0"), "{stdout}");
    assert!(temp.path().join("out/missing.md").is_file());
}

#[test]
fn missing_without_fail_p0_exits_zero() {
    let temp = TempDir::new("missing-ok");
    temp.write(
        "out/symbols.toml",
        "schema = 1\n[[symbol]]\nname = \"strlcpy\"\nring = 1\nring_computed = 1\nring_source = \"computed\"\n",
    );
    temp.write(
        "out/ports.json",
        "{\"snapshots\":{\"aports\":{\"packages\":1}},\"packages\":{},\"symbols\":{\"strlcpy\":3},\"headers\":{},\"macros\":{}}",
    );
    temp.write("implemented.txt", "");
    let result = command(
        &temp,
        &[
            "missing",
            "--implemented",
            "implemented.txt",
            "--out",
            "out",
        ],
    );
    assert_eq!(result.status.code(), Some(0));
}
