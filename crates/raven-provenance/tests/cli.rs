// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! End-to-end tests of the `raven-provenance` binary.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

use serde_json::Value;

const BIN: &str = env!("CARGO_BIN_EXE_raven-provenance");
const HEADER: &str = "// SPDX-License-Identifier: BSD-3-Clause\n\
                      // Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors\n\n";
const IGNORE: &str =
    "Cargo.lock\nLICENSE\nNOTICE\nPROVENANCE.toml\nAI_POLICY.md\nREADME.md\ndocs/man\n";

static COUNTER: AtomicU64 = AtomicU64::new(0);

struct TempDir {
    path: PathBuf,
}

impl TempDir {
    fn new(label: &str) -> Self {
        let mut path = std::env::temp_dir();
        path.push(format!(
            "raven-provenance-cli-{label}-{}-{}",
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

    fn repo(label: &str) -> Self {
        let temp = Self::new(label);
        temp.write(".provenance-ignore", IGNORE);
        temp.write("PROVENANCE.toml", "# no imports\n");
        temp
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

fn run(root: &Path, args: &[&str]) -> std::process::Output {
    Command::new(BIN)
        .arg(args[0])
        .arg("--root")
        .arg(root)
        .args(&args[1..])
        .output()
        .expect("run raven-provenance")
}

#[test]
fn clean_tree_exits_zero() {
    let temp = TempDir::repo("clean");
    temp.write("src/main.rs", &format!("{HEADER}fn main() {{}}\n"));
    let output = run(temp.path(), &["check"]);
    assert_eq!(output.status.code(), Some(0));
    assert!(output.stdout.is_empty());
}

#[test]
fn missing_header_exits_one() {
    let temp = TempDir::repo("bad");
    temp.write("src/bad.rs", "fn main() {}\n");
    let output = run(temp.path(), &["check"]);
    assert_eq!(output.status.code(), Some(1));
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("spdx-missing"), "{stdout}");
}

#[test]
fn json_output_has_the_contract_fields() {
    let temp = TempDir::repo("json");
    temp.write("src/main.rs", &format!("{HEADER}fn main() {{}}\n"));
    let output = run(temp.path(), &["check", "--json"]);
    assert_eq!(output.status.code(), Some(0));
    let value: Value = serde_json::from_slice(&output.stdout).expect("valid JSON");
    assert!(value["violations"].is_array());
    assert!(value["warnings"].is_array());
    assert!(value["files_checked"].is_u64());
    assert!(value["files_checked"].as_u64().unwrap() >= 1);
}

#[test]
fn usage_errors_exit_two() {
    let output = Command::new(BIN).output().expect("run raven-provenance");
    assert_eq!(output.status.code(), Some(2));
    let output = Command::new(BIN)
        .arg("nonsense")
        .output()
        .expect("run raven-provenance");
    assert_eq!(output.status.code(), Some(2));
}

#[test]
fn fix_spdx_then_check_passes_and_is_idempotent() {
    let temp = TempDir::repo("fix");
    temp.write("src/main.rs", "fn main() {}\n");
    temp.write("scripts/run.sh", "#!/bin/sh\necho ok\n");
    temp.write("docs/design.md", "# Design\n");
    let output = run(temp.path(), &["fix-spdx"]);
    assert_eq!(output.status.code(), Some(0));
    assert!(temp.read("scripts/run.sh").starts_with("#!/bin/sh\n# SPDX"));
    assert!(temp.read("docs/design.md").starts_with("<!-- SPDX"));

    let output = run(temp.path(), &["check"]);
    assert_eq!(output.status.code(), Some(0));

    let before = temp.read("src/main.rs");
    let output = run(temp.path(), &["fix-spdx"]);
    assert_eq!(output.status.code(), Some(0));
    assert_eq!(temp.read("src/main.rs"), before);
}

#[test]
fn check_does_not_modify_files() {
    let temp = TempDir::repo("readonly");
    let content = "fn main() {}\n";
    temp.write("src/bad.rs", content);
    let output = run(temp.path(), &["check"]);
    assert_eq!(output.status.code(), Some(1));
    assert_eq!(temp.read("src/bad.rs"), content);
}
