// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

use std::process::Command;

use super::*;
use crate::testutil::TempDir;

fn required() -> bool {
    std::env::var_os("RL_ORACLE_REQUIRED").is_some_and(|value| value == "1")
}

fn skip(reason: &str) {
    if required() {
        panic!("RL_ORACLE_REQUIRED=1 but {reason}");
    }
    println!("SKIP: {reason}");
}

fn git(dir: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .output()
        .expect("run git");
    assert!(
        output.status.success(),
        "git {}: {}",
        args.join(" "),
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).trim().to_string()
}

fn repo(temp: &TempDir, content: &str) -> (std::path::PathBuf, String) {
    let checkout = temp.directory("donor");
    git(&checkout, &["init", "-q"]);
    temp.write("donor/lib/file.c", content);
    git(&checkout, &["add", "lib/file.c"]);
    git(
        &checkout,
        &[
            "-c",
            "user.name=Oracle Test",
            "-c",
            "user.email=oracle@example.invalid",
            "commit",
            "-qm",
            "fixture",
        ],
    );
    let sha = git(&checkout, &["rev-parse", "HEAD"]);
    (checkout, sha)
}

fn write_lock(temp: &TempDir, sha: &str) {
    temp.write(
        "donors.lock",
        &format!("donor  {sha}  A  https://example.invalid/donor.git\n"),
    );
}

#[test]
fn matching_head_and_clean_file_pass() {
    if !git_available() {
        skip("git not found");
        return;
    }
    let temp = TempDir::new("pin-ok");
    let (_, sha) = repo(&temp, "int f(void) { return 1; }\n");
    write_lock(&temp, &sha);
    check(temp.path(), "donor", "lib/file.c").expect("pin");
}

#[test]
fn a_mismatching_head_refuses() {
    if !git_available() {
        skip("git not found");
        return;
    }
    let temp = TempDir::new("pin-head");
    repo(&temp, "int f(void) { return 1; }\n");
    write_lock(&temp, &"0".repeat(40));
    let error = check(temp.path(), "donor", "lib/file.c").unwrap_err();
    assert!(error.to_string().contains("donors.lock pins"));
}

#[test]
fn a_modified_file_refuses() {
    if !git_available() {
        skip("git not found");
        return;
    }
    let temp = TempDir::new("pin-dirty");
    let (_, sha) = repo(&temp, "int f(void) { return 1; }\n");
    write_lock(&temp, &sha);
    temp.write("donor/lib/file.c", "int f(void) { return 2; }\n");
    let error = check(temp.path(), "donor", "lib/file.c").unwrap_err();
    assert!(error.to_string().contains("is modified"));
}

#[test]
fn an_unlisted_donor_refuses() {
    if !git_available() {
        skip("git not found");
        return;
    }
    let temp = TempDir::new("pin-unlisted");
    let (_, sha) = repo(&temp, "int f(void) { return 1; }\n");
    write_lock(&temp, &sha);
    let error = check(temp.path(), "other", "lib/file.c").unwrap_err();
    assert!(error.to_string().contains("not in donors.lock"));
}

#[test]
fn a_malformed_lock_row_is_a_recipe_error() {
    let temp = TempDir::new("pin-malformed");
    temp.write("donors.lock", "donor  not-a-sha  A  url\n");
    let error = read_lock(temp.path()).unwrap_err();
    assert!(error.to_string().contains("40-hex"));
}
