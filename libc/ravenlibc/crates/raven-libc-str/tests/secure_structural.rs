// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Structural constant-time evidence for ticket 5.4 (b) and (c).
//!
//! A timing measurement on the host is meaningless (caches, SMT, the OS); the claim is proved on
//! the emitted code: the secure loops must have no conditional branch **inside the loop body**
//! other than the single backward loop edge, and the barrier probe must keep stores that the
//! plain fill loses.

#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::indexing_slicing,
    clippy::panic
)]

use std::path::{Path, PathBuf};
use std::process::Command;

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

fn build() -> PathBuf {
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
        "cargo rustc failed:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let archive = target_dir().join("release/libraven_libc_str.rlib");
    assert!(archive.is_file(), "missing {}", archive.display());
    archive
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

fn disassemble(archive: &Path) -> String {
    if let Some(tool) = sysroot_tool("llvm-objdump") {
        let output = Command::new(tool)
            .args(["-d", "--demangle", &archive.display().to_string()])
            .output()
            .expect("llvm-objdump");
        assert!(output.status.success(), "llvm-objdump failed");
        return String::from_utf8_lossy(&output.stdout).into_owned();
    }
    if let Some(tool) = sysroot_tool("objdump").or_else(|| {
        Command::new("objdump")
            .arg("--version")
            .output()
            .ok()
            .map(|_| "objdump".to_string())
    }) {
        let output = Command::new(tool)
            .args(["-d", "-C", &archive.display().to_string()])
            .output()
            .expect("objdump");
        assert!(output.status.success(), "objdump failed");
        return String::from_utf8_lossy(&output.stdout).into_owned();
    }
    panic!("neither llvm-objdump nor objdump is available");
}

fn block(text: &str, needle: &str) -> Vec<String> {
    let mut result = Vec::new();
    let mut inside = false;
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.contains('<') && trimmed.ends_with(">:") {
            inside = trimmed.contains(needle);
            continue;
        }
        if inside {
            if trimmed.is_empty() {
                break;
            }
            result.push(trimmed.to_string());
        }
    }
    result
}

fn instruction(line: &str) -> Option<(u64, String)> {
    let (address, rest) = line.split_once(':')?;
    let address = u64::from_str_radix(address.trim(), 16).ok()?;
    let mut fields = rest.split_whitespace();
    // Skip the raw instruction bytes of GNU/LLVM objdump.
    let mut mnemonic = None;
    for field in &mut fields {
        if field.len() > 1 && field.chars().all(|c| c.is_ascii_hexdigit()) {
            continue;
        }
        mnemonic = Some(field.to_string());
        break;
    }
    Some((address, mnemonic?))
}

fn branch_target(line: &str) -> Option<u64> {
    let (_, rest) = line.split_once(':')?;
    let fields: Vec<&str> = rest.split_whitespace().collect();
    let mut index = 0usize;
    while index < fields.len()
        && fields[index].len() > 1
        && fields[index].chars().all(|c| c.is_ascii_hexdigit())
    {
        index = index.wrapping_add(1);
    }
    // `fields[index]` is the mnemonic; the branch target follows it.
    let target = fields.get(index.wrapping_add(1))?;
    let clean = target.trim_start_matches("0x");
    if clean.is_empty() || !clean.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    u64::from_str_radix(clean, 16).ok()
}

fn is_conditional(mnemonic: &str) -> bool {
    if matches!(
        mnemonic,
        "jmp" | "b" | "ret" | "retq" | "call" | "callq" | "bl" | "jal"
    ) {
        return false;
    }
    mnemonic.starts_with('j')
        || mnemonic == "loop"
        || mnemonic.starts_with("b.")
        || mnemonic.starts_with("cb")
        || mnemonic.starts_with("tb")
        || mnemonic.starts_with("beq")
        || mnemonic.starts_with("bne")
        || mnemonic.starts_with("blt")
        || mnemonic.starts_with("bge")
}

fn check_loop(name: &str, body: &[String]) {
    // The compiler emits a top-tested loop: one conditional branch (the loop bound) and one
    // unconditional backward edge. A data-dependent early exit would need a *second* conditional
    // branch; the test forbids it. The trip count depends only on the two slice lengths.
    let mut conditional = Vec::new();
    let mut backward = 0usize;
    for line in body {
        let Some((address, mnemonic)) = instruction(line) else {
            continue;
        };
        let branch = is_conditional(&mnemonic) || mnemonic == "jmp";
        if !branch {
            continue;
        }
        if let Some(target) = branch_target(line)
            && target < address
        {
            backward = backward.wrapping_add(1);
        }
        if is_conditional(&mnemonic) {
            conditional.push(line.clone());
        }
    }
    assert_eq!(
        conditional.len(),
        1,
        "{name}: expected exactly one conditional branch (the loop bound), got {conditional:?}"
    );
    assert!(backward >= 1, "{name}: no backward loop edge found");
    println!(
        "{name}: one conditional loop-bound branch, one backward edge; no data-dependent branch"
    );
}

#[test]
fn secure_engines_have_no_data_dependent_branch() {
    let archive = build();
    let text = disassemble(&archive);
    for name in [
        "raven_libc_str::secure::timingsafe_bcmp",
        "raven_libc_str::secure::timingsafe_memcmp",
    ] {
        let body = block(&text, name);
        assert!(!body.is_empty(), "{name} not found");
        check_loop(name, &body);
    }
}

fn store_count(body: &[String]) -> usize {
    body.iter()
        .filter(|line| {
            let Some((_, mnemonic)) = instruction(line) else {
                return false;
            };
            let store_like = mnemonic.starts_with("mov")
                || mnemonic.starts_with("vmov")
                || mnemonic == "stos"
                || mnemonic.starts_with("stos");
            if !store_like {
                return false;
            }
            // AT&T: the last operand is the destination; llvm-objdump prints AT&T for x86 here.
            line.rsplit(',')
                .next()
                .is_some_and(|operand| operand.contains('('))
        })
        .count()
}

#[test]
fn barrier_probe_keeps_stores() {
    let archive = build();
    let text = disassemble(&archive);
    let with = block(
        &text,
        "raven_libc_str::raw::probes::probe_zero_with_barrier",
    );
    let without = block(
        &text,
        "raven_libc_str::raw::probes::probe_zero_without_barrier",
    );
    assert!(!with.is_empty(), "with-barrier probe not found");
    assert!(!without.is_empty(), "without-barrier probe not found");
    let with_stores = store_count(&with);
    let without_stores = store_count(&without);
    println!("probe store counts: with barrier {with_stores}, without barrier {without_stores}");
    assert!(
        with_stores > without_stores,
        "barrier probe has {with_stores} stores, plain fill {without_stores}"
    );
}
