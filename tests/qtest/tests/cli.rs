// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! End-to-end tests of the `raven-qtest` binary using `--runner`.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

use serde_json::Value;

const BIN: &str = env!("CARGO_BIN_EXE_raven-qtest");
static COUNTER: AtomicU64 = AtomicU64::new(0);

struct TempDir {
    path: PathBuf,
}

impl TempDir {
    fn new(label: &str) -> Self {
        let mut path = std::env::temp_dir();
        path.push(format!(
            "raven-qtest-cli-{label}-{}-{}",
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
            fs::create_dir_all(parent).expect("create parent dir");
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

fn basic_case(name: &str, timeout: u64, expect_exit: &str) -> String {
    format!(
        "name = \"{name}\"\narch = \"x86_64\"\nprofile = \"base\"\n\
         timeout_secs = {timeout}\nexpect_exit = \"{expect_exit}\"\n"
    )
}

fn run(case: &Path, out: &Path, script: &str) -> Output {
    Command::new(BIN)
        .args(["run", "--case"])
        .arg(case)
        .args(["--out"])
        .arg(out)
        .args(["--runner", "sh", "-c", script])
        .output()
        .expect("run raven-qtest")
}

#[test]
fn passing_runner_case_writes_log_and_json() {
    let temp = TempDir::new("pass");
    let case = temp.path().join("passing.toml");
    temp.write(
        "passing.toml",
        &format!(
            "{}[[expect]]\nregex = \"^hello$\"\n[[expect]]\nregex = \"^world$\"\n",
            basic_case("passing", 5, "success")
        ),
    );
    let out = temp.path().join("out");
    let result = run(&case, &out, "printf 'hello\\nworld\\n'; exit 33");
    assert_eq!(result.status.code(), Some(0));
    let stdout = String::from_utf8_lossy(&result.stdout);
    assert!(stdout.contains("passing: pass"), "{stdout}");

    assert_eq!(temp.read("out/passing.log"), "hello\nworld\n");
    let json: Value = serde_json::from_str(&temp.read("out/passing.json")).unwrap();
    assert_eq!(json["case"], "passing");
    assert_eq!(json["arch"], "x86_64");
    assert_eq!(json["result"], "pass");
    assert_eq!(json["exit_status"], 33);
    assert!(json["duration_ms"].is_u64());
    assert_eq!(json["matched"].as_array().unwrap().len(), 2);
    assert!(json["unmatched"].as_array().unwrap().is_empty());
    assert!(json["forbidden_hits"].as_array().unwrap().is_empty());
}

#[test]
fn unmatched_expectation_fails_with_serial_tail() {
    let temp = TempDir::new("fail");
    let case = temp.path().join("failing.toml");
    temp.write(
        "failing.toml",
        &format!(
            "{}[[expect]]\nregex = \"^missing$\"\n",
            basic_case("failing", 5, "success")
        ),
    );
    let out = temp.path().join("out");
    let result = run(&case, &out, "printf 'other\\n'; exit 33");
    assert_eq!(result.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&result.stderr);
    assert!(stderr.contains("unmatched expectations"), "{stderr}");
    assert!(stderr.contains("serial"), "{stderr}");
    assert!(stderr.contains("other"), "{stderr}");

    let json: Value = serde_json::from_str(&temp.read("out/failing.json")).unwrap();
    assert_eq!(json["result"], "fail");
    assert_eq!(json["unmatched"][0], "^missing$");
}

#[test]
fn expected_failure_exit_passes() {
    let temp = TempDir::new("failure");
    let case = temp.path().join("failure.toml");
    temp.write("failure.toml", &basic_case("failure", 5, "failure"));
    let out = temp.path().join("out");
    let result = run(&case, &out, "exit 35");
    assert_eq!(result.status.code(), Some(0));
    let json: Value = serde_json::from_str(&temp.read("out/failure.json")).unwrap();
    assert_eq!(json["result"], "pass");
    assert_eq!(json["exit_status"], 35);
}

#[test]
fn timeout_is_enforced_and_reported() {
    let temp = TempDir::new("timeout");
    let case = temp.path().join("slow.toml");
    temp.write("slow.toml", &basic_case("slow", 1, "success"));
    let out = temp.path().join("out");
    let started = Instant::now();
    let result = run(&case, &out, "sleep 5");
    let elapsed = started.elapsed();
    assert_eq!(result.status.code(), Some(1));
    assert!(elapsed.as_millis() < 3000, "took {elapsed:?}");

    let json: Value = serde_json::from_str(&temp.read("out/slow.json")).unwrap();
    assert_eq!(json["result"], "timeout");
    assert!(json["exit_status"].is_null());
    assert!(json["duration_ms"].as_u64().unwrap() < 3000);
}

#[test]
fn forbid_hit_fails_the_case() {
    let temp = TempDir::new("forbid");
    let case = temp.path().join("forbid.toml");
    temp.write(
        "forbid.toml",
        &format!(
            "{}[[forbid]]\nregex = \"panic\"\n",
            basic_case("forbid", 5, "success")
        ),
    );
    let out = temp.path().join("out");
    let result = run(&case, &out, "printf 'panic here\\n'; exit 33");
    assert_eq!(result.status.code(), Some(1));
    let json: Value = serde_json::from_str(&temp.read("out/forbid.json")).unwrap();
    assert_eq!(json["result"], "fail");
    assert_eq!(json["forbidden_hits"][0], "panic here");
}

#[test]
fn crlf_and_ansi_are_stripped_before_matching() {
    let temp = TempDir::new("ansi");
    let case = temp.path().join("ansi.toml");
    temp.write(
        "ansi.toml",
        &format!(
            "{}[[expect]]\nregex = \"^hello$\"\n[[expect]]\nregex = \"^world$\"\n",
            basic_case("ansi", 5, "success")
        ),
    );
    let out = temp.path().join("out");
    let result = run(
        &case,
        &out,
        "printf 'hello\\r\\n\\033[31mworld\\033[0m\\n'; exit 33",
    );
    assert_eq!(result.status.code(), Some(0));
}

#[test]
fn skipped_case_exits_77() {
    let temp = TempDir::new("skip");
    let case = temp.path().join("skip.toml");
    temp.write(
        "skip.toml",
        "name = \"skip\"\narch = \"riscv64\"\nprofile = \"base\"\ntimeout_secs = 5\n\
         expect_exit = \"success\"\n",
    );
    let image = temp.path().join("dummy.img");
    fs::write(&image, b"image").unwrap();
    let out = temp.path().join("out");
    let result = Command::new(BIN)
        .args(["run", "--case"])
        .arg(&case)
        .args(["--image"])
        .arg(&image)
        .args(["--out"])
        .arg(&out)
        .output()
        .expect("run raven-qtest");
    assert_eq!(result.status.code(), Some(77));
    let json: Value = serde_json::from_str(&temp.read("out/skip.json")).unwrap();
    assert_eq!(json["result"], "skipped");
    let stdout = String::from_utf8_lossy(&result.stdout);
    assert!(stdout.contains("skipped"), "{stdout}");
}

#[test]
fn usage_errors_exit_two() {
    let result = Command::new(BIN).args(["run"]).output().unwrap();
    assert_eq!(result.status.code(), Some(2));

    let temp = TempDir::new("usage");
    let case = temp.path().join("case.toml");
    temp.write("case.toml", &basic_case("usage", 5, "success"));
    let result = Command::new(BIN)
        .args(["run", "--case"])
        .arg(&case)
        .arg("--runner")
        .output()
        .unwrap();
    assert_eq!(result.status.code(), Some(2));
}

#[test]
fn invalid_case_is_a_usage_error() {
    let temp = TempDir::new("badcase");
    let case = temp.path().join("bad.toml");
    temp.write(
        "bad.toml",
        &format!("{}mystery = 1\n", basic_case("bad", 5, "success")),
    );
    let result = Command::new(BIN)
        .args(["run", "--case"])
        .arg(&case)
        .output()
        .unwrap();
    assert_eq!(result.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&result.stderr);
    assert!(stderr.contains("unknown field"), "{stderr}");
}

#[test]
fn list_handles_an_empty_directory() {
    let temp = TempDir::new("list-empty");
    temp.write("cases/.keep", "");
    let result = Command::new(BIN)
        .args(["list", "--cases"])
        .arg(temp.path().join("cases"))
        .output()
        .unwrap();
    assert_eq!(result.status.code(), Some(0));
    assert!(result.stdout.is_empty());
}

#[test]
fn list_prints_case_names() {
    let temp = TempDir::new("list");
    temp.write("cases/one.toml", &basic_case("one", 5, "success"));
    temp.write("cases/two.toml", &basic_case("two", 5, "success"));
    let result = Command::new(BIN)
        .args(["list", "--cases"])
        .arg(temp.path().join("cases"))
        .output()
        .unwrap();
    assert_eq!(result.status.code(), Some(0));
    let stdout = String::from_utf8_lossy(&result.stdout);
    assert!(stdout.contains("one"), "{stdout}");
    assert!(stdout.contains("two"), "{stdout}");
}

#[test]
fn run_accepts_no_build_without_a_prebuilt_image() {
    let temp = TempDir::new("no-build");
    let case = temp.path().join("no-build.toml");
    temp.write("no-build.toml", &basic_case("no-build", 5, "success"));
    let out = temp.path().join("out");
    let result = Command::new(BIN)
        .args(["run", "--case"])
        .arg(&case)
        .args(["--out"])
        .arg(&out)
        .arg("--no-build")
        .output()
        .expect("run raven-qtest");
    assert_eq!(
        result.status.code(),
        Some(1),
        "stderr: {}",
        String::from_utf8_lossy(&result.stderr)
    );
    let stdout = String::from_utf8_lossy(&result.stdout);
    let stderr = String::from_utf8_lossy(&result.stderr);
    assert!(stdout.contains("image not found (--no-build)"), "{stdout}");
    assert!(!stdout.contains("unexpected argument"), "{stdout}");
    assert!(!stderr.contains("unexpected argument"), "{stderr}");
    let json: Value = serde_json::from_str(&temp.read("out/no-build.json")).unwrap();
    assert_eq!(json["result"], "error");
    assert_eq!(json["case"], "no-build");
}

#[test]
fn run_rejects_unknown_flags() {
    let temp = TempDir::new("unknown-flag");
    let case = temp.path().join("case.toml");
    temp.write("case.toml", &basic_case("unknown", 5, "success"));
    let result = Command::new(BIN)
        .args(["run", "--case"])
        .arg(&case)
        .arg("--wat")
        .output()
        .expect("run raven-qtest");
    assert_eq!(result.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&result.stderr);
    assert!(stderr.contains("unexpected argument `--wat`"), "{stderr}");
}

#[test]
fn help_documents_no_build_on_the_run_line() {
    let result = Command::new(BIN)
        .arg("--help")
        .output()
        .expect("run raven-qtest");
    assert_eq!(result.status.code(), Some(0));
    let stdout = String::from_utf8_lossy(&result.stdout);
    assert!(
        stdout
            .lines()
            .any(|line| line.contains("run --case") && line.contains("[--no-build]")),
        "{stdout}"
    );
}

#[test]
fn step_dialogue_passes_through_the_cli() {
    let temp = TempDir::new("dialogue-pass");
    let case = temp.path().join("dialogue.toml");
    temp.write(
        "dialogue.toml",
        &format!(
            "{}[[step]]\nwait = '^ready$'\nsend = \"uname\\n\"\ndelay_ms = 0\n\
             [[expect]]\nregex = '^got: uname$'\n",
            basic_case("dialogue", 5, "success")
        ),
    );
    let out = temp.path().join("out");
    let result = run(
        &case,
        &out,
        "printf 'ready\\n'; read line; printf 'got: %s\\n' \"$line\"; exit 33",
    );
    assert_eq!(
        result.status.code(),
        Some(0),
        "stderr: {}",
        String::from_utf8_lossy(&result.stderr)
    );
    let stdout = String::from_utf8_lossy(&result.stdout);
    assert!(stdout.contains("dialogue: pass"), "{stdout}");
    assert_eq!(temp.read("out/dialogue.log"), "ready\ngot: uname\n");
    let json: Value = serde_json::from_str(&temp.read("out/dialogue.json")).unwrap();
    assert_eq!(json["result"], "pass");
}

#[test]
fn step_failure_exits_one_and_is_printed() {
    let temp = TempDir::new("dialogue-fail");
    let case = temp.path().join("stuck.toml");
    temp.write(
        "stuck.toml",
        &format!(
            "{}[[step]]\nwait = '^never$'\ntimeout_secs = 1\n",
            basic_case("stuck", 5, "success")
        ),
    );
    let out = temp.path().join("out");
    let result = run(&case, &out, "printf 'started\\n'; sleep 5");
    assert_eq!(result.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&result.stderr);
    assert!(stderr.contains("step 1"), "{stderr}");
    assert!(stderr.contains("^never$"), "{stderr}");
    assert!(stderr.contains("started"), "{stderr}");
    let json: Value = serde_json::from_str(&temp.read("out/stuck.json")).unwrap();
    assert_eq!(json["result"], "fail");
}
