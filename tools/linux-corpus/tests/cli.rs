// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! End-to-end tests of the `linux-corpus` binary on small temporary corpora.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

use serde_json::Value;

const BIN: &str = env!("CARGO_BIN_EXE_linux-corpus");
static COUNTER: AtomicU64 = AtomicU64::new(0);

struct TempDir {
    path: PathBuf,
}

impl TempDir {
    fn new(label: &str) -> Self {
        let mut path = std::env::temp_dir();
        path.push(format!(
            "linux-corpus-cli-{label}-{}-{}",
            std::process::id(),
            COUNTER.fetch_add(1, Ordering::SeqCst)
        ));
        fs::create_dir_all(&path).expect("create temp dir");
        Self { path }
    }

    fn path(&self) -> &Path {
        &self.path
    }

    fn write(&self, relative: &str, content: &str) {
        let path = self.path.join(relative);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("create parent");
        }
        fs::write(path, content).expect("write test file");
    }

    fn read(&self, relative: &str) -> String {
        fs::read_to_string(self.path.join(relative)).expect("read test file")
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

fn corpus_with_one(temp: &TempDir) -> PathBuf {
    temp.write(
        "corpus.toml",
        "[[program]]\nid = \"one\"\ncategory = \"shell\"\n\
         argv = [\"sh\", \"-c\", \"printf 'hello\\\\n'\"]\n",
    );
    temp.path().join("corpus.toml")
}

fn run_success(temp: &TempDir, corpus: &Path) -> PathBuf {
    let out = temp.path().join("out");
    let result = Command::new(BIN)
        .args(["run", "--corpus"])
        .arg(corpus)
        .args(["--out"])
        .arg(&out)
        .output()
        .expect("run linux-corpus");
    assert_eq!(
        result.status.code(),
        Some(0),
        "stderr: {}",
        String::from_utf8_lossy(&result.stderr)
    );
    out
}

#[test]
fn run_writes_trace_and_json() {
    let temp = TempDir::new("run");
    let corpus = corpus_with_one(&temp);
    let out = run_success(&temp, &corpus);
    assert!(out.join("one.strace").is_file());
    let json: Value = serde_json::from_str(&temp.read("out/one.json")).unwrap();
    assert_eq!(json["id"], "one");
    assert_eq!(json["exit"], 0);
    assert_eq!(json["skipped"], false);
    assert!(json["syscalls"].as_object().unwrap().contains_key("write"));
}

#[test]
fn run_skips_a_missing_requirement() {
    let temp = TempDir::new("skip");
    temp.write(
        "corpus.toml",
        "[[program]]\nid = \"skip\"\ncategory = \"shell\"\n\
         requires = [\"definitely-not-a-real-binary-b05\"]\nargv = [\"true\"]\n",
    );
    let out = run_success(&temp, &temp.path().join("corpus.toml"));
    let json: Value = serde_json::from_str(&temp.read("out/skip.json")).unwrap();
    assert_eq!(json["skipped"], true);
    assert!(json["reason"].as_str().unwrap().contains("not installed"));
    assert!(!out.join("skip.strace").is_file());
}

#[test]
fn run_unknown_only_id_is_a_usage_error() {
    let temp = TempDir::new("unknown");
    let corpus = corpus_with_one(&temp);
    let result = Command::new(BIN)
        .args(["run", "--corpus"])
        .arg(corpus)
        .args(["--only", "nope"])
        .output()
        .unwrap();
    assert_eq!(result.status.code(), Some(2));
}

#[test]
fn merge_writes_json_and_markdown() {
    let temp = TempDir::new("merge");
    let corpus = corpus_with_one(&temp);
    let out = run_success(&temp, &corpus);
    let json_path = temp.path().join("coverage.json");
    let md_path = temp.path().join("coverage.md");
    let result = Command::new(BIN)
        .args(["merge", "--out"])
        .arg(&out)
        .args(["--json"])
        .arg(&json_path)
        .args(["--md"])
        .arg(&md_path)
        .output()
        .unwrap();
    assert_eq!(
        result.status.code(),
        Some(0),
        "stderr: {}",
        String::from_utf8_lossy(&result.stderr)
    );
    let json: Value = serde_json::from_str(&temp.read("coverage.json")).unwrap();
    assert_eq!(json["programs"][0]["id"], "one");
    assert!(
        json["syscalls"]
            .as_array()
            .unwrap()
            .iter()
            .any(|entry| entry["name"] == "write")
    );
    let md = temp.read("coverage.md");
    assert!(md.contains("Top 60 syscalls"));
    assert!(md.contains("### shell"));
}

#[test]
fn missing_filters_and_ranks_syscalls() {
    let temp = TempDir::new("missing");
    let corpus = corpus_with_one(&temp);
    let out = run_success(&temp, &corpus);
    temp.write("impl.txt", "read\n");
    let result = Command::new(BIN)
        .args(["missing", "--out"])
        .arg(&out)
        .args(["--implemented"])
        .arg(temp.path().join("impl.txt"))
        .output()
        .unwrap();
    assert_eq!(result.status.code(), Some(0));
    let stdout = String::from_utf8_lossy(&result.stdout);
    assert!(stdout.contains("write"), "{stdout}");
}

#[test]
fn usage_errors_exit_two() {
    let result = Command::new(BIN).output().unwrap();
    assert_eq!(result.status.code(), Some(2));
    let result = Command::new(BIN)
        .args(["merge", "--bogus"])
        .output()
        .unwrap();
    assert_eq!(result.status.code(), Some(2));
}

#[test]
fn merge_with_implemented_writes_the_missing_section() {
    let temp = TempDir::new("merge-impl");
    let corpus = corpus_with_one(&temp);
    let out = run_success(&temp, &corpus);
    temp.write("impl.txt", "read\n");
    let md = temp.path().join("coverage.md");
    let result = Command::new(BIN)
        .args(["merge", "--out"])
        .arg(&out)
        .args(["--json"])
        .arg(temp.path().join("coverage.json"))
        .args(["--md"])
        .arg(&md)
        .args(["--implemented"])
        .arg(temp.path().join("impl.txt"))
        .output()
        .unwrap();
    assert_eq!(result.status.code(), Some(0));
    let text = temp.read("coverage.md");
    assert!(text.contains("## Not yet implemented"));
    assert!(text.contains("`write`"));
}
