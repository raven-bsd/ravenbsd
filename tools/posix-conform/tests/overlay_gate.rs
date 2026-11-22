// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! End-to-end gate checks for the owner evidence overlay.
//!
//! Each test creates a minimal profile, a temporary qtest case or shell test, and a fake `cargo`
//! that answers the qtest invocations. The gate really executes the claim; no QEMU is needed.

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::Value;

fn root(label: &str) -> PathBuf {
    let base =
        std::env::temp_dir().join(format!("posix-overlay-gate-{label}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&base);
    let path = base.join("root");
    fs::create_dir_all(base.join("ravenbsd-prjmgmt/architecture/product-data/posix/evidence"))
        .unwrap();
    fs::create_dir_all(&path).unwrap();
    fs::create_dir_all(path.join("tests/qtest/cases")).unwrap();
    fs::create_dir_all(path.join("tests/posix")).unwrap();
    fs::create_dir_all(path.join("bin")).unwrap();
    path
}

fn posix(root: &Path) -> PathBuf {
    root.parent()
        .unwrap()
        .join("ravenbsd-prjmgmt/architecture/product-data/posix")
}

fn chmod_exec(path: &Path) {
    let mut permissions = fs::metadata(path).unwrap().permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(path, permissions).unwrap();
}

fn write_min_profile(root: &Path) {
    fs::write(
        posix(root).join("profile.toml"),
        "schema = 1\nstandard = \"posix-2024\"\n\n\
         [[process]]\n\
         id = \"process.fork\"\n\
         standard = \"posix-2024\"\n\
         status = \"partial\"\n\
         evidence = \"none\"\n\
         owner = \"P5\"\n\
         waiver = \"\"\n\
         notes = \"\"\n\
         pg = []\n\
         name = \"fork\"\n\
         description = \"fork\"\n",
    )
    .unwrap();
    fs::write(
        posix(root).join("traceability.toml"),
        "schema = 1\nstandard = \"posix-2024\"\n\n\
         [[trace]]\n\
         id = \"process.fork\"\n\
         ticket = \"P5\"\n\
         test = \"none\"\n\
         evidence = \"none\"\n",
    )
    .unwrap();
}

fn write_overlay(root: &Path, owner: &str, row: &str) {
    fs::write(
        posix(root).join("evidence").join(format!("{owner}.toml")),
        format!("schema = 1\nstandard = \"posix-2024\"\n\n{row}"),
    )
    .unwrap();
}

fn write_fake_cargo(root: &Path) {
    let path = root.join("bin/cargo");
    fs::write(
        &path,
        "#!/bin/sh\n\
         case \"$*\" in\n\
           *\"overlay-fail\"*) echo \"fake qtest failure\"; exit 1 ;;\n\
           *) echo \"fake qtest pass\"; exit 0 ;;\n\
         esac\n",
    )
    .unwrap();
    chmod_exec(&path);
}

fn run_gate(root: &Path) -> Value {
    let bin = env!("CARGO_BIN_EXE_posix-conform");
    let output = Command::new(bin)
        .arg("gate")
        .arg("--root")
        .arg(root)
        .arg("--profile")
        .arg("posix")
        .arg("--arch")
        .arg("x86_64")
        .arg("--json")
        .env(
            "PATH",
            format!("{}:/usr/bin:/bin", root.join("bin").display()),
        )
        .env_remove("RAVEN_MGMT_ROOT")
        .output()
        .unwrap();
    serde_json::from_slice(&output.stdout).unwrap_or_else(|error| {
        panic!(
            "gate did not return JSON: {error}\nstdout:\n{}\nstderr:\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        )
    })
}

fn row<'a>(report: &'a Value, id: &str) -> &'a Value {
    report["rows"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["id"] == id)
        .unwrap_or_else(|| panic!("missing row {id} in {report}"))
}

fn cleanup(path: &Path) {
    if let Some(base) = path.parent() {
        let _ = fs::remove_dir_all(base);
    }
}

#[test]
fn valid_qtest_evidence_passes() {
    let path = root("valid-qtest");
    write_min_profile(&path);
    write_fake_cargo(&path);
    fs::write(
        path.join("tests/qtest/cases/overlay-pass.toml"),
        "name = \"overlay-pass\"\n",
    )
    .unwrap();
    write_overlay(
        &path,
        "P5",
        "[[row]]\nid = \"process.fork\"\nstatus = \"present\"\n\
         evidence = \"qtest:overlay-pass\"\nnotes = \"qtest claim\"\n",
    );
    let report = run_gate(&path);
    let result = row(&report, "process.fork");
    assert_eq!(result["result"], "pass", "{report}");
    assert_eq!(result["command"], "qtest:overlay-pass");
    cleanup(&path);
}

#[test]
fn failing_qtest_evidence_fails() {
    let path = root("failing-qtest");
    write_min_profile(&path);
    write_fake_cargo(&path);
    fs::write(
        path.join("tests/qtest/cases/overlay-fail.toml"),
        "name = \"overlay-fail\"\n",
    )
    .unwrap();
    write_overlay(
        &path,
        "P5",
        "[[row]]\nid = \"process.fork\"\nstatus = \"present\"\n\
         evidence = \"qtest:overlay-fail\"\nnotes = \"qtest claim\"\n",
    );
    let report = run_gate(&path);
    let result = row(&report, "process.fork");
    assert_eq!(result["result"], "fail", "{report}");
    assert!(
        result["reason"]
            .as_str()
            .unwrap()
            .contains("fake qtest failure"),
        "{report}"
    );
    cleanup(&path);
}

#[test]
fn missing_qtest_evidence_fails() {
    let path = root("missing-qtest");
    write_min_profile(&path);
    write_fake_cargo(&path);
    write_overlay(
        &path,
        "P5",
        "[[row]]\nid = \"process.fork\"\nstatus = \"present\"\n\
         evidence = \"qtest:no-such-case\"\nnotes = \"qtest claim\"\n",
    );
    let report = run_gate(&path);
    let result = row(&report, "process.fork");
    assert_eq!(result["result"], "fail", "{report}");
    assert!(
        result["reason"]
            .as_str()
            .unwrap()
            .contains("does not exist"),
        "{report}"
    );
    cleanup(&path);
}

#[test]
fn fake_test_path_fails() {
    let path = root("fake-test");
    write_min_profile(&path);
    write_fake_cargo(&path);
    write_overlay(
        &path,
        "P5",
        "[[row]]\nid = \"process.fork\"\nstatus = \"present\"\n\
         evidence = \"test:tests/posix/does-not-exist.sh\"\nnotes = \"test claim\"\n",
    );
    let report = run_gate(&path);
    let result = row(&report, "process.fork");
    assert_eq!(result["result"], "fail", "{report}");
    assert!(
        result["reason"]
            .as_str()
            .unwrap()
            .contains("does not exist"),
        "{report}"
    );
    cleanup(&path);
}

#[test]
fn valid_test_evidence_passes() {
    let path = root("valid-test");
    write_min_profile(&path);
    write_fake_cargo(&path);
    fs::write(path.join("tests/posix/pass.sh"), "#!/bin/sh\nexit 0\n").unwrap();
    write_overlay(
        &path,
        "P5",
        "[[row]]\nid = \"process.fork\"\nstatus = \"present\"\n\
         evidence = \"test:tests/posix/pass.sh\"\nnotes = \"test claim\"\n",
    );
    let report = run_gate(&path);
    let result = row(&report, "process.fork");
    assert_eq!(result["result"], "pass", "{report}");
    assert_eq!(result["command"], "test:tests/posix/pass.sh");
    cleanup(&path);
}

#[test]
fn failing_test_evidence_fails() {
    let path = root("failing-test");
    write_min_profile(&path);
    write_fake_cargo(&path);
    fs::write(
        path.join("tests/posix/fail.sh"),
        "#!/bin/sh\necho \"test failed\" >&2\nexit 1\n",
    )
    .unwrap();
    write_overlay(
        &path,
        "P5",
        "[[row]]\nid = \"process.fork\"\nstatus = \"present\"\n\
         evidence = \"test:tests/posix/fail.sh\"\nnotes = \"test claim\"\n",
    );
    let report = run_gate(&path);
    let result = row(&report, "process.fork");
    assert_eq!(result["result"], "fail", "{report}");
    assert!(
        result["reason"].as_str().unwrap().contains("test failed"),
        "{report}"
    );
    cleanup(&path);
}

#[test]
fn unknown_id_fails() {
    let path = root("unknown-id");
    write_min_profile(&path);
    write_fake_cargo(&path);
    fs::write(
        path.join("tests/qtest/cases/overlay-pass.toml"),
        "name = \"overlay-pass\"\n",
    )
    .unwrap();
    write_overlay(
        &path,
        "P5",
        "[[row]]\nid = \"process.nope\"\nstatus = \"present\"\n\
         evidence = \"qtest:overlay-pass\"\nnotes = \"unknown\"\n",
    );
    let report = run_gate(&path);
    let result = row(&report, "process.nope");
    assert_eq!(result["result"], "fail", "{report}");
    assert!(
        result["reason"]
            .as_str()
            .unwrap()
            .contains("no profile row"),
        "{report}"
    );
    cleanup(&path);
}

#[test]
fn owner_mismatch_fails() {
    let path = root("owner-mismatch");
    write_min_profile(&path);
    write_fake_cargo(&path);
    fs::write(
        path.join("tests/qtest/cases/overlay-pass.toml"),
        "name = \"overlay-pass\"\n",
    )
    .unwrap();
    write_overlay(
        &path,
        "P4",
        "[[row]]\nid = \"process.fork\"\nstatus = \"present\"\n\
         evidence = \"qtest:overlay-pass\"\nnotes = \"wrong owner\"\n",
    );
    let report = run_gate(&path);
    let result = row(&report, "process.fork");
    assert_eq!(result["result"], "fail", "{report}");
    assert!(
        result["reason"]
            .as_str()
            .unwrap()
            .contains("owner mismatch"),
        "{report}"
    );
    cleanup(&path);
}

#[test]
fn waived_claim_passes_and_stays_visible() {
    let path = root("waived");
    write_min_profile(&path);
    write_fake_cargo(&path);
    write_overlay(
        &path,
        "P5",
        "[[row]]\nid = \"process.fork\"\nstatus = \"waived\"\n\
         evidence = \"none\"\nnotes = \"OWNER WAIVER: P5 omits fork for the first gate\"\n",
    );
    let report = run_gate(&path);
    let result = row(&report, "process.fork");
    assert_eq!(result["result"], "pass", "{report}");
    assert_eq!(result["command"], "waiver");
    assert!(
        result["reason"].as_str().unwrap().contains("OWNER WAIVER"),
        "{report}"
    );
    cleanup(&path);
}
