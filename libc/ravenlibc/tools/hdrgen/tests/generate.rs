// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Integration tests for `generate` and `check`.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const GROUPS: [&str; 14] = [
    "errno", "signal", "fcntl", "mman", "stat", "socket", "netinet", "poll", "time", "resource",
    "limits", "sysconf", "pthread", "misc",
];

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../..")
}

fn resources() -> PathBuf {
    let mut directory = Some(repo_root());
    while let Some(current) = directory {
        let candidate = current.join("ravenbsd-resources");
        if candidate.join("donors.lock").is_file() {
            return candidate;
        }
        directory = current.parent().map(Path::to_path_buf);
    }
    panic!("cannot find ravenbsd-resources");
}

struct Temp {
    path: PathBuf,
}

impl Temp {
    fn new(tag: &str) -> Self {
        let path = std::env::temp_dir().join(format!(
            "hdrgen-it-{tag}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |duration| duration.as_nanos())
        ));
        std::fs::create_dir_all(&path).expect("temp dir");
        Self { path }
    }

    fn abi(&self) -> PathBuf {
        self.path.join("abi")
    }

    fn rust(&self) -> PathBuf {
        self.path.join("rust")
    }

    /// Copy the 17 hand-written TOML files into a fresh abi directory.
    fn copy_tables(&self) -> PathBuf {
        let abi = self.abi();
        std::fs::create_dir_all(&abi).expect("abi dir");
        for name in ["types.toml", "structs.toml", "deviations.toml"] {
            std::fs::copy(
                repo_root().join("libc/ravenlibc/abi").join(name),
                abi.join(name),
            )
            .expect("copy table");
        }
        for group in GROUPS {
            let name = format!("{group}.toml");
            std::fs::copy(
                repo_root().join("libc/ravenlibc/abi").join(&name),
                abi.join(name),
            )
            .expect("copy group");
        }
        abi
    }
}

impl Drop for Temp {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

fn hdrgen(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_raven-libc-hdrgen"))
        .args(args)
        .output()
        .expect("run raven-libc-hdrgen")
}

fn strings(path: &Path) -> Vec<(PathBuf, String)> {
    let mut files = Vec::new();
    fn walk(directory: &Path, files: &mut Vec<(PathBuf, String)>) {
        let Ok(entries) = std::fs::read_dir(directory) else {
            return;
        };
        let mut paths: Vec<PathBuf> = entries.flatten().map(|entry| entry.path()).collect();
        paths.sort();
        for path in paths {
            if path.is_dir() {
                walk(&path, files);
            } else if let Ok(text) = std::fs::read_to_string(&path) {
                files.push((path, text));
            }
        }
    }
    walk(path, &mut files);
    files
}

#[test]
fn generate_twice_is_byte_identical() {
    let temp = Temp::new("twice");
    let abi = temp.copy_tables();
    let rust = temp.rust();
    let root = repo_root();
    let res = resources();
    let args = [
        "generate",
        "--abi",
        abi.to_str().unwrap(),
        "--rust",
        rust.to_str().unwrap(),
        "--root",
        root.to_str().unwrap(),
        "--resources",
        res.to_str().unwrap(),
    ];
    let first = hdrgen(&args);
    assert!(
        first.status.success(),
        "{}",
        String::from_utf8_lossy(&first.stderr)
    );
    let files = strings(&temp.path);
    let generated = files
        .iter()
        .filter(|(path, _)| path.starts_with(&rust) || path.starts_with(abi.join("generated")))
        .count();
    assert_eq!(generated, 35, "generated file count");
    let second = hdrgen(&args);
    assert!(second.status.success());
    assert_eq!(files, strings(&temp.path));
}

#[test]
fn generate_writes_linux_syscall_numbers() {
    let temp = Temp::new("syscalls");
    let abi = temp.copy_tables();
    let rust = temp.rust();
    let root = repo_root();
    let res = resources();
    let output = hdrgen(&[
        "generate",
        "--abi",
        abi.to_str().unwrap(),
        "--rust",
        rust.to_str().unwrap(),
        "--root",
        root.to_str().unwrap(),
        "--resources",
        res.to_str().unwrap(),
    ]);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let x86 = std::fs::read_to_string(abi.join("generated/include/arch/x86_64/bits/syscall.h"))
        .expect("x86_64 syscall header");
    let arm = std::fs::read_to_string(abi.join("generated/include/arch/aarch64/bits/syscall.h"))
        .expect("aarch64 syscall header");
    assert!(x86.contains("#define __NR_read 0\n"));
    assert!(arm.contains("#define __NR_read 63\n"));
    assert!(x86.contains("#define SYS_read __NR_read\n"));
}

#[test]
fn check_accepts_a_freshly_generated_tree() {
    let temp = Temp::new("fresh");
    let abi = temp.copy_tables();
    let rust = temp.rust();
    let root = repo_root();
    let res = resources();
    let args = [
        "generate",
        "--abi",
        abi.to_str().unwrap(),
        "--rust",
        rust.to_str().unwrap(),
        "--root",
        root.to_str().unwrap(),
        "--resources",
        res.to_str().unwrap(),
    ];
    assert!(hdrgen(&args).status.success());
    let check = hdrgen(&[
        "check",
        "--abi",
        abi.to_str().unwrap(),
        "--rust",
        rust.to_str().unwrap(),
        "--root",
        root.to_str().unwrap(),
        "--resources",
        res.to_str().unwrap(),
    ]);
    assert!(
        check.status.success(),
        "{}",
        String::from_utf8_lossy(&check.stderr)
    );
}

#[test]
fn check_fails_on_an_edited_generated_file() {
    let temp = Temp::new("edited");
    let abi = temp.copy_tables();
    let rust = temp.rust();
    let root = repo_root();
    let res = resources();
    let common = [
        "--abi",
        abi.to_str().unwrap(),
        "--rust",
        rust.to_str().unwrap(),
        "--root",
        root.to_str().unwrap(),
        "--resources",
        res.to_str().unwrap(),
    ];
    let mut generate = vec!["generate"];
    generate.extend(common.iter().copied());
    assert!(hdrgen(&generate).status.success());
    let victim = rust.join("errno.rs");
    let mut text = std::fs::read_to_string(&victim).expect("errno.rs");
    text.push_str("// edited\n");
    std::fs::write(&victim, text).expect("write");
    let mut check = vec!["check"];
    check.extend(common.iter().copied());
    let output = hdrgen(&check);
    assert_eq!(output.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("stale generated file"), "{stderr}");
    assert!(stderr.contains("errno.rs"), "{stderr}");
}

#[test]
fn check_fails_on_a_missing_generated_file() {
    let temp = Temp::new("missing");
    let abi = temp.copy_tables();
    let rust = temp.rust();
    let root = repo_root();
    let res = resources();
    let common = [
        "--abi",
        abi.to_str().unwrap(),
        "--rust",
        rust.to_str().unwrap(),
        "--root",
        root.to_str().unwrap(),
        "--resources",
        res.to_str().unwrap(),
    ];
    let mut generate = vec!["generate"];
    generate.extend(common.iter().copied());
    assert!(hdrgen(&generate).status.success());
    std::fs::remove_file(rust.join("types.rs")).expect("remove");
    let mut check = vec!["check"];
    check.extend(common.iter().copied());
    let output = hdrgen(&check);
    assert_eq!(output.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&output.stderr).contains("missing generated file"));
}

#[test]
fn check_fails_on_an_extra_generated_file() {
    let temp = Temp::new("extra");
    let abi = temp.copy_tables();
    let rust = temp.rust();
    let root = repo_root();
    let res = resources();
    let common = [
        "--abi",
        abi.to_str().unwrap(),
        "--rust",
        rust.to_str().unwrap(),
        "--root",
        root.to_str().unwrap(),
        "--resources",
        res.to_str().unwrap(),
    ];
    let mut generate = vec!["generate"];
    generate.extend(common.iter().copied());
    assert!(hdrgen(&generate).status.success());
    std::fs::write(rust.join("extra.rs"), "// extra\n").expect("write");
    let mut check = vec!["check"];
    check.extend(common.iter().copied());
    let output = hdrgen(&check);
    assert_eq!(output.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&output.stderr).contains("extra generated file"));
}

#[test]
fn check_passes_on_the_committed_tree() {
    let output = hdrgen(&[
        "check",
        "--root",
        repo_root().to_str().unwrap(),
        "--resources",
        resources().to_str().unwrap(),
    ]);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(String::from_utf8_lossy(&output.stderr).contains("check ok"));
}
