// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The recursion gate and the `nm -u` check of ticket 7.
//!
//! The test builds the crate as an `rlib` (the `staticlib` form needs the one panic handler
//! that C60 owns), then
//! 1. asserts that the only undefined symbols are the block primitives LLVM may emit for
//!    patterns the crate itself does not use, and
//! 2. disassembles `raw::{memcpy, memmove, memset, memcmp, bcmp, strlen, strcpy, stpcpy}` and
//!    asserts that none of them calls one of those names (a `jmp` tail call to a different
//!    routine is allowed).
//!
//! `RL_NO_OBJDUMP_OK=1` downgrades a missing disassembler to a reported skip (the report must
//! say so); it never downgrades a failed check.

#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::indexing_slicing,
    clippy::panic
)]

use std::path::{Path, PathBuf};
use std::process::Command;

const FORBIDDEN: &[&str] = &["memcpy", "memmove", "memset", "memcmp", "bcmp", "strlen"];
const CHECKED: &[&str] = &[
    "memcpy", "memmove", "memset", "memcmp", "bcmp", "strlen", "strcpy", "stpcpy",
];

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(4)
        .expect("workspace root")
        .to_path_buf()
}

fn target_dir() -> PathBuf {
    std::env::var_os("CARGO_TARGET_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| workspace_root().join("target"))
}

fn build_staticlib() -> PathBuf {
    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".to_string());
    let output = Command::new(cargo)
        .args([
            "rustc",
            "-p",
            "raven-libc-str",
            "--release",
            "--crate-type",
            "lib",
            "--features",
            "test-probes",
        ])
        .current_dir(workspace_root())
        .output()
        .expect("run cargo rustc");
    assert!(
        output.status.success(),
        "cargo rustc failed:\n{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let archive = target_dir().join("release/libraven_libc_str.rlib");
    assert!(archive.is_file(), "missing {}", archive.display());
    archive
}

fn tool(name: &str) -> Option<String> {
    // The rustup llvm-tools component keeps LLVM-matched binaries next to rustc; the system
    // copy may be too old to read this toolchain's bitcode.
    if let Some(candidate) = sysroot_tool(name) {
        return Some(candidate);
    }
    if Command::new(name).arg("--version").output().is_ok() {
        return Some(name.to_string());
    }
    None
}

fn sysroot_tool(name: &str) -> Option<String> {
    let sysroot = Command::new("rustc")
        .args(["--print", "sysroot"])
        .output()
        .ok()?;
    let sysroot = String::from_utf8_lossy(&sysroot.stdout).trim().to_string();
    let host = Command::new("rustc").args(["-vV"]).output().ok()?;
    let host = String::from_utf8_lossy(&host.stdout)
        .lines()
        .find_map(|line| line.strip_prefix("host: ").map(str::to_string))?;
    let candidate = Path::new(&sysroot)
        .join("lib")
        .join("rustlib")
        .join(host)
        .join("bin")
        .join(name);
    candidate.is_file().then(|| candidate.display().to_string())
}

fn nm_undefined(archive: &Path) -> Vec<String> {
    let nm = tool("llvm-nm").or_else(|| tool("nm")).expect("nm");
    let output = Command::new(nm)
        .args(["-u", &archive.display().to_string()])
        .output()
        .expect("nm -u");
    assert!(output.status.success(), "nm -u failed");
    let mut undefined = Vec::new();
    for line in String::from_utf8_lossy(&output.stdout).lines() {
        let line = line.trim();
        if line.is_empty() || line.ends_with(':') {
            continue;
        }
        let Some(name) = line.split_whitespace().last() else {
            continue;
        };
        if !undefined.iter().any(|seen| seen == name) {
            undefined.push(name.to_string());
        }
    }
    undefined.sort();
    undefined
}

#[test]
fn undefined_symbols_are_only_the_block_primitives() {
    let archive = build_staticlib();
    let undefined = nm_undefined(&archive);
    println!("nm -u {}: {undefined:?}", archive.display());
    for symbol in &undefined {
        let base = symbol.trim_start_matches('_');
        assert!(
            FORBIDDEN.contains(&base) || base.starts_with("__rust_"),
            "unexpected undefined symbol `{symbol}`"
        );
    }
}

fn blocks(disassembly: &str) -> Vec<(String, Vec<String>)> {
    let mut result: Vec<(String, Vec<String>)> = Vec::new();
    for line in disassembly.lines() {
        let trimmed = line.trim();
        let header = trimmed
            .find('<')
            .zip(trimmed.rfind(">:"))
            .filter(|(start, end)| end > start);
        if let Some((start, end)) = header {
            result.push((trimmed[start + 1..end].to_string(), Vec::new()));
            continue;
        }
        if let Some((_, body)) = result.last_mut() {
            body.push(line.to_string());
        }
    }
    result
}

fn disassemble(archive: &Path) -> String {
    if let Some(tool) = tool("llvm-objdump") {
        let output = Command::new(tool)
            .args(["-d", "--demangle", &archive.display().to_string()])
            .output()
            .expect("llvm-objdump");
        assert!(output.status.success(), "llvm-objdump failed");
        return String::from_utf8_lossy(&output.stdout).into_owned();
    }
    if let Some(tool) = tool("objdump") {
        let output = Command::new(tool)
            .args(["-d", "-C", &archive.display().to_string()])
            .output()
            .expect("objdump");
        assert!(output.status.success(), "objdump failed");
        return String::from_utf8_lossy(&output.stdout).into_owned();
    }
    if std::env::var("RL_NO_OBJDUMP_OK").as_deref() == Ok("1") {
        println!("no objdump/llvm-objdump on PATH: disassembly check skipped (reported)");
        return String::new();
    }
    panic!("neither llvm-objdump nor objdump is available (set RL_NO_OBJDUMP_OK=1 to skip)");
}

#[test]
fn the_eight_bodies_do_not_call_themselves() {
    let archive = build_staticlib();
    let text = disassemble(&archive);
    if text.is_empty() {
        return;
    }
    let mut checked = Vec::new();
    for (name, body) in blocks(&text) {
        let Some(function) = CHECKED.iter().find(|function| {
            name.contains("raven_libc_str::raw::")
                && (name.ends_with(&format!("::{function}"))
                    || name.ends_with(&format!("::{function}::h")))
        }) else {
            continue;
        };
        checked.push((*function).to_string());
        for line in &body {
            let line = line.trim();
            let is_call =
                line.starts_with("call") || line.starts_with("bl") || line.starts_with("jal");
            if !is_call {
                continue;
            }
            for forbidden in FORBIDDEN {
                assert!(
                    !line.contains(forbidden),
                    "{} calls {}: {line}",
                    name,
                    forbidden
                );
            }
        }
    }
    println!("disassembly checked: {checked:?}");
    for function in CHECKED {
        assert!(
            checked.iter().any(|seen| seen == function),
            "function {function} not found in the staticlib disassembly"
        );
    }
}
