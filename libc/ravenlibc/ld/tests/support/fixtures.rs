// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Shared fixture builder for the C62 host tests.
//!
//! The sources are assembler text (`*.s.txt` so the `no-cc` lint accepts them); the builder
//! invokes `clang` and the pinned `rust-lld`. A missing tool prints `SKIP:` and returns.

#![allow(dead_code, clippy::collapsible_if, clippy::if_same_then_else)]

use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::OnceLock;

pub const X86_64_TARGET: &str = "x86_64-unknown-linux-gnu";

fn rust_lld() -> Option<PathBuf> {
    if let Ok(value) = env::var("LD_LLD") {
        return Some(PathBuf::from(value));
    }
    if let Ok(output) = Command::new("rustc").arg("--print").arg("sysroot").output() {
        if output.status.success() {
            let root = String::from_utf8_lossy(&output.stdout).trim().to_string();
            let host = host_triple();
            let candidate = PathBuf::from(root)
                .join("lib/rustlib")
                .join(host)
                .join("bin/rust-lld");
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }
    let candidate = PathBuf::from("ld.lld");
    if Command::new(&candidate)
        .arg("--version")
        .output()
        .is_ok_and(|output| output.status.success())
    {
        return Some(candidate);
    }
    None
}

fn host_triple() -> String {
    "x86_64-unknown-linux-gnu".to_string()
}

fn clang() -> Option<PathBuf> {
    if let Ok(value) = env::var("CLANG") {
        return Some(PathBuf::from(value));
    }
    let candidate = PathBuf::from("clang");
    if Command::new(&candidate)
        .arg("--version")
        .output()
        .is_ok_and(|output| output.status.success())
    {
        Some(candidate)
    } else {
        None
    }
}

/// The fixture directory, built once per test binary.
pub fn fixtures() -> Option<PathBuf> {
    static CACHE: OnceLock<Option<PathBuf>> = OnceLock::new();
    CACHE
        .get_or_init(|| match build_fixtures() {
            Ok(path) => Some(path),
            Err(error) => {
                eprintln!("fixtures: {error}");
                None
            }
        })
        .clone()
}

fn build_fixtures() -> Result<PathBuf, String> {
    let clang = clang().ok_or_else(|| "SKIP: clang".to_string())?;
    let lld = rust_lld().ok_or_else(|| "SKIP: ld.lld".to_string())?;
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let source = root.join("tests/fixtures/x86_64");
    let out = env::temp_dir().join(format!("c62-fixtures-{}", std::process::id()));
    let _ = fs::remove_dir_all(&out);
    fs::create_dir_all(&out).map_err(|error| error.to_string())?;
    for name in [
        "libdeep",
        "libval",
        "libver",
        "libifunc",
        "libweak",
        "libhidden",
        "libcycle_b",
        "libcycle_a",
    ] {
        compile(&clang, &lld, &source, &out, name)?;
    }
    Ok(out)
}

fn compile(clang: &Path, lld: &Path, source: &Path, out: &Path, name: &str) -> Result<(), String> {
    let asm = source.join(format!("{name}.s.txt"));
    if !asm.is_file() {
        return Ok(());
    }
    let object = out.join(format!("{name}.o"));
    let status = Command::new(clang)
        .args(["-target", X86_64_TARGET, "-c", "-x", "assembler"])
        .arg(&asm)
        .arg("-o")
        .arg(&object)
        .status()
        .map_err(|error| error.to_string())?;
    if !status.success() {
        return Err(format!("clang failed for {name}"));
    }
    let soname = format!("{name}.so");
    let library = out.join(&soname);
    let mut command = Command::new(lld);
    command.args([
        "-flavor",
        "gnu",
        "-shared",
        "-z",
        "now",
        "-z",
        "relro",
        "-z",
        "pack-relative-relocs",
        "--hash-style=gnu",
        "-soname",
        &soname,
        "-o",
    ]);
    command.arg(&library).arg(&object);
    if name == "libval" {
        command.arg(out.join("libdeep.so"));
    }
    if name == "libcycle_a" {
        command.arg(out.join("libcycle_b.so"));
    }
    if name == "libver" {
        let script = out.join("libver.map");
        fs::write(
            &script,
            "LIBV_0.9 { global: ver; };\nLIBV_1.0 { global: ver; } LIBV_0.9;\n",
        )
        .map_err(|error| error.to_string())?;
        command.arg("--version-script").arg(&script);
    }
    let status = command.status().map_err(|error| error.to_string())?;
    if !status.success() {
        return Err(format!("rust-lld failed for {name}"));
    }
    Ok(())
}

/// Build a tiny x86-64 stub library exporting `names` as zero-returning functions.
///
/// The host `libc.so.1` is a partial link: its kernel/stdio imports are still undefined. A
/// host loader test supplies those addresses from this stub, which is what a real process
/// would get from the rest of the system.
pub fn build_stub(names: &[String]) -> Option<PathBuf> {
    let clang = clang()?;
    let lld = rust_lld()?;
    let out = env::temp_dir().join(format!("c62-stub-{}", std::process::id()));
    let _ = fs::remove_dir_all(&out);
    fs::create_dir_all(&out).ok()?;
    let source = out.join("stub.s");
    let mut text = String::from(".text\n");
    for name in names {
        if !name
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || character == '_')
        {
            continue;
        }
        text.push_str(&format!(
            ".globl {name}\n.type {name},@function\n{name}:\n        xor %eax, %eax\n        ret\n"
        ));
    }
    text.push_str(".section .note.GNU-stack,\"\",@progbits\n");
    fs::write(&source, text).ok()?;
    let object = out.join("stub.o");
    let status = Command::new(clang)
        .args(["-target", X86_64_TARGET, "-c", "-x", "assembler"])
        .arg(&source)
        .arg("-o")
        .arg(&object)
        .status()
        .ok()?;
    if !status.success() {
        return None;
    }
    let library = out.join("libhoststub.so");
    let status = Command::new(lld)
        .args([
            "-flavor",
            "gnu",
            "-shared",
            "-z",
            "now",
            "-z",
            "relro",
            "--hash-style=gnu",
            "-soname",
            "libhoststub.so",
            "-o",
        ])
        .arg(&library)
        .arg(&object)
        .status()
        .ok()?;
    status.success().then_some(library)
}

/// The fixture directory or a printed skip.
pub fn required() -> Option<PathBuf> {
    match fixtures() {
        Some(path) => Some(path),
        None => {
            println!("SKIP: clang or rust-lld not found");
            if env::var("RL_RTLD_REQUIRED").ok().as_deref() == Some("1") {
                panic!("RL_RTLD_REQUIRED=1 and the fixture tools are missing");
            }
            None
        }
    }
}
