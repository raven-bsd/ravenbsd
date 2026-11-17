// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! CLI acceptance tests for the `libc-world` binary.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("repo root")
}

fn tool() -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_libc-world"));
    command.current_dir(repo_root());
    command
}

struct Output {
    code: i32,
    stdout: String,
    stderr: String,
}

fn run(mut command: Command) -> Output {
    let output = command.output().expect("run");
    Output {
        code: output.status.code().unwrap_or(-1),
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
    }
}

fn temp(name: &str) -> PathBuf {
    let path = std::env::temp_dir().join(format!(
        "libc-world-cli-{name}-{}-{:?}",
        std::process::id(),
        std::thread::current().id()
    ));
    let _ = fs::remove_dir_all(&path);
    fs::create_dir_all(&path).expect("temp");
    path
}

#[test]
fn list_smoke5_prints_the_build_order() {
    let output = run({
        let mut command = tool();
        command.args(["list", "--subset", "smoke5"]);
        command
    });
    assert_eq!(output.code, 0, "{}", output.stderr);
    assert_eq!(output.stdout.trim(), "zlib\nlua\nsqlite\npcre2\ntoybox");
}

#[test]
fn list_tier_one_contains_the_compression_rows() {
    let output = run({
        let mut command = tool();
        command.args(["list", "--tier", "1"]);
        command
    });
    assert_eq!(output.code, 0, "{}", output.stderr);
    assert!(output.stdout.lines().any(|line| line == "gzip"));
    assert!(output.stdout.lines().any(|line| line == "libc-test"));
}

#[test]
fn check_accepts_the_committed_world() {
    let output = run({
        let mut command = tool();
        command.arg("check");
        command
    });
    assert_eq!(output.code, 0, "{}", output.stderr);
    assert!(output.stdout.contains("200 packages"));
    assert!(output.stdout.contains("197.5 weighted"));
    assert!(output.stdout.contains("freeze pending"));
}

#[test]
fn check_strict_refuses_a_pending_world_with_exit_one() {
    let output = run({
        let mut command = tool();
        command.args(["check", "--strict"]);
        command
    });
    assert_eq!(output.code, 1);
    assert!(output.stderr.contains("--strict"));
}

#[test]
fn check_tarballs_skips_placeholder_rows() {
    let temp = temp("check-tarballs");
    let output = run({
        let mut command = tool();
        command.args(["check", "--tarballs"]);
        command.arg(&temp);
        command
    });
    assert_eq!(output.code, 0, "{}", output.stderr);
}

#[test]
fn an_unknown_command_is_exit_two() {
    let output = run({
        let mut command = tool();
        command.arg("nonsense");
        command
    });
    assert_eq!(output.code, 2);
    assert!(output.stderr.contains("unknown command"));
}

#[test]
fn build_without_a_sysroot_names_libc_pack() {
    let temp = temp("no-sysroot");
    let output = run({
        let mut command = tool();
        command.arg("build").arg("zlib").arg("--sysroot");
        command.arg(temp.join("nope"));
        command
    });
    assert_eq!(output.code, 2);
    assert!(output.stderr.contains("libc-pack"));
}

#[test]
fn gate_accepts_the_met_fixture() {
    let fixture = repo_root().join("tools/libc-world/tests/fixtures/results-met");
    let output = run({
        let mut command = tool();
        command.arg("gate").arg("--results").arg(fixture);
        command.args(["--min-built", "3"]);
        command
    });
    assert_eq!(output.code, 0, "{}", output.stderr);
    assert!(output.stdout.contains("C13 world build: 3 of 3"));
    assert!(output.stdout.contains("-> MET"));
}

#[test]
fn gate_refuses_the_patched_fixture() {
    let fixture = repo_root().join("tools/libc-world/tests/fixtures/results-patched");
    let output = run({
        let mut command = tool();
        command.arg("gate").arg("--results").arg(fixture);
        command.args(["--min-built", "3"]);
        command
    });
    assert_eq!(output.code, 1);
    assert!(output.stdout.contains("libc_patches 1"));
    assert!(output.stdout.contains("package `lua`"));
}

#[test]
fn gate_refuses_an_empty_results_directory() {
    let temp = temp("gate-empty");
    let output = run({
        let mut command = tool();
        command.arg("gate").arg("--results").arg(&temp);
        command
    });
    assert_eq!(output.code, 1);
    assert!(output.stdout.contains("no package results"));
}

#[test]
fn report_writes_world_files() {
    let temp = temp("report");
    let results = temp.join("results/x86_64");
    fs::create_dir_all(&results).expect("results");
    for name in ["zlib", "lua", "sqlite"] {
        fs::copy(
            repo_root()
                .join("tools/libc-world/tests/fixtures/results-met")
                .join(format!("{name}.json")),
            results.join(format!("{name}.json")),
        )
        .expect("copy");
    }
    let output = run({
        let mut command = tool();
        command
            .arg("report")
            .arg("--results")
            .arg(temp.join("results"));
        command.args(["--arch", "x86_64"]);
        command
    });
    assert_eq!(output.code, 0, "{}", output.stderr);
    assert!(results.join("world.json").is_file());
    assert!(results.join("world.md").is_file());
    let md = fs::read_to_string(results.join("world.md")).expect("md");
    assert!(md.contains("| 25 | zlib |"));
}

#[test]
fn missing_prints_the_ranked_backlog() {
    let temp = temp("missing");
    let arch = temp.join("x86_64");
    fs::create_dir_all(arch.join("undef")).expect("undef");
    fs::write(arch.join("undef/zlib.undef"), "strlcpy\tU\n").expect("undef");
    let output = run({
        let mut command = tool();
        command.arg("missing").arg("--results").arg(&temp);
        command.args(["--arch", "x86_64"]);
        command
    });
    assert_eq!(output.code, 0, "{}", output.stderr);
    assert!(output.stdout.contains("strlcpy\t1\t1\t0"));
    assert!(output.stdout.contains("libc-census"));
}

fn fill_zlib_row(world: &str) -> String {
    let mut parts: Vec<String> = world.split("[[package]]").map(str::to_string).collect();
    for part in &mut parts {
        if part.contains("name = \"zlib\"") {
            *part = part
                .replace("version = \"PLACEHOLDER\"", "version = \"1.0\"")
                .replace(
                    "url = \"PLACEHOLDER\"",
                    "url = \"https://example.invalid/zlib-1.0.tar.gz\"",
                )
                .replace(
                    "sha256 = \"PLACEHOLDER\"",
                    "sha256 = \"0000000000000000000000000000000000000000000000000000000000000000\"",
                )
                .replace("tarball = \"PLACEHOLDER\"", "tarball = \"zlib-1.0.tar.gz\"");
        }
    }
    parts.join("[[package]]")
}

#[test]
fn a_missing_tarball_is_data_not_an_error() {
    let temp = temp("tarball-missing");
    let world_path = temp.join("world.toml");
    let world = fs::read_to_string(repo_root().join("tools/libc-world/world.toml")).expect("world");
    fs::write(&world_path, fill_zlib_row(&world)).expect("scratch world");
    let sysroot = temp.join("sysroot");
    fs::create_dir_all(&sysroot).expect("sysroot");
    let tarballs = temp.join("tarballs");
    fs::create_dir_all(&tarballs).expect("tarballs");
    let bin = temp.join("bin");
    fs::create_dir_all(&bin).expect("bin");
    let lld = bin.join("ld.lld");
    fs::write(&lld, "#!/bin/sh\nexit 0\n").expect("lld");
    use std::os::unix::fs::PermissionsExt as _;
    let mut permissions = fs::metadata(&lld).expect("meta").permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(&lld, permissions).expect("chmod");
    let path = format!(
        "{}:{}",
        bin.display(),
        std::env::var("PATH").unwrap_or_default()
    );
    let output = run({
        let mut command = tool();
        command.arg("build").arg("zlib");
        command.arg("--world").arg(&world_path);
        command.arg("--sysroot").arg(&sysroot);
        command.arg("--tarballs").arg(&tarballs);
        command.arg("--out").arg(temp.join("out"));
        command.arg("--results").arg(temp.join("results"));
        command.env("PATH", path);
        command
    });
    assert_eq!(output.code, 0, "{}", output.stderr);
    assert!(output.stdout.contains("zlib\ttarball-missing"));
    let result = fs::read_to_string(temp.join("results/x86_64/zlib.json")).expect("result");
    assert!(result.contains("\"status\":\"tarball-missing\""));
}

#[test]
fn help_prints_the_usage() {
    let output = run({
        let mut command = tool();
        command.arg("--help");
        command
    });
    assert_eq!(output.code, 0);
    assert!(output.stdout.contains("libc-world build"));
}
