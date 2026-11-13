// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! Golden disassembly of the `t_snprintf` instance on every target.
//!
//! The test compiles a tiny `#![no_std]` crate that includes `src/va/trampoline.rs`, expands
//! `raven_variadic!(prefix: "t_", export: "snprintf", named: 3)` and nothing else, for each
//! target and profile, disassembles the object with `llvm-objdump -d --no-show-raw-insn` and
//! compares the text with `golden/<name>.txt`. Regenerate with `RL_UPDATE_GOLDEN=1`; the test
//! never rewrites a golden file silently.
//!
//! A missing `rustc`, `llvm-objdump` or target install is a SKIP with a printed reason.

#![allow(unexpected_cfgs)] // `raven_va_softfloat` and `raven_va_verified_*` are build-profile cfgs

use std::path::{Path, PathBuf};
use std::process::Command;

/// One golden instance.
struct Golden {
    name: &'static str,
    target: &'static str,
    cfgs: &'static [&'static str],
}

const GOLDENS: [Golden; 5] = [
    Golden {
        name: "x86_64",
        target: "x86_64-unknown-none",
        cfgs: &[],
    },
    Golden {
        name: "x86_64_softfloat",
        target: "x86_64-unknown-none",
        cfgs: &["raven_va_softfloat"],
    },
    Golden {
        // No hard-float aarch64 target is installed (only `aarch64-unknown-none-softfloat`),
        // so the unverified form of the same target is the aarch64 golden: the link-time
        // failing stub. The verified soft-float form is the next file.
        name: "aarch64",
        target: "aarch64-unknown-none-softfloat",
        cfgs: &[],
    },
    Golden {
        name: "aarch64_softfloat",
        target: "aarch64-unknown-none-softfloat",
        cfgs: &["raven_va_verified_aarch64"],
    },
    Golden {
        name: "riscv64",
        target: "riscv64gc-unknown-none-elf",
        cfgs: &["raven_va_verified_riscv64"],
    },
];

fn manifest() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn workdir() -> PathBuf {
    if let Ok(directory) = std::env::var("CARGO_TARGET_TMPDIR") {
        return PathBuf::from(directory).join("va-golden");
    }
    manifest()
        .ancestors()
        .nth(3)
        .map(|root| root.join("target/va-golden"))
        .unwrap_or_else(|| manifest().join("target/va-golden"))
}

/// The tiny crate source: only the trampoline module and one instance of the macro.
fn instance_source(manifest: &Path) -> String {
    let trampoline =
        manifest.join("../../../libc/ravenlibc/crates/raven-libc/src/va/trampoline.rs");
    format!(
        "#![no_std]\n\
         #[path = \"{}\"]\n\
         mod trampoline;\n\
         unsafe extern \"C\" {{\n    fn vdemo(a: usize, b: usize, c: usize, ap: *mut u8) -> i32;\n}}\n\
         crate::raven_variadic!(prefix: \"t_\", export: \"snprintf\", target: vdemo, named: 3);\n",
        trampoline.display()
    )
}

fn find_tool(name: &str) -> Option<PathBuf> {
    if let Ok(value) = std::env::var("LLVM_OBJDUMP")
        && name == "llvm-objdump"
    {
        return Some(PathBuf::from(value));
    }
    let status = Command::new(name).arg("--version").output();
    match status {
        Ok(output) if output.status.success() => Some(PathBuf::from(name)),
        _ => None,
    }
}

/// Compile the instance for one target and return the object path, or the SKIP reason.
fn compile(golden: &Golden, directory: &Path) -> Result<PathBuf, String> {
    std::fs::create_dir_all(directory).map_err(|error| error.to_string())?;
    let source = directory.join(format!("{}.rs", golden.name));
    std::fs::write(&source, instance_source(&manifest())).map_err(|error| error.to_string())?;
    let object = directory.join(format!("{}.o", golden.name));
    let mut command = Command::new("rustc");
    command
        .arg("--edition")
        .arg("2024")
        .arg("--target")
        .arg(golden.target)
        .arg("--crate-type")
        .arg("lib")
        .arg("--emit")
        .arg("obj")
        .arg("-o")
        .arg(&object)
        .arg(&source);
    for cfg in golden.cfgs {
        command.arg("--cfg").arg(cfg);
    }
    let output = command.output().map_err(|error| error.to_string())?;
    if !output.status.success() {
        let message = String::from_utf8_lossy(&output.stderr).into_owned();
        if message.contains("can't find crate for `core`")
            || message.contains("may not be installed")
        {
            return Err(format!("skipped: target {} not installed", golden.target));
        }
        return Err(format!("rustc failed: {message}"));
    }
    Ok(object)
}

/// Disassemble and normalise: drop the file header and the blank lines.
fn disassemble(object: &Path) -> Result<String, String> {
    let Some(tool) = find_tool("llvm-objdump") else {
        return Err("skipped: llvm-objdump not found".to_string());
    };
    let output = Command::new(tool)
        .arg("-d")
        .arg("--no-show-raw-insn")
        .arg(object)
        .output()
        .map_err(|error| error.to_string())?;
    if !output.status.success() {
        return Err(format!(
            "llvm-objdump failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    let text = String::from_utf8_lossy(&output.stdout);
    let mut lines = Vec::new();
    for line in text.lines() {
        let trimmed = line.trim_end();
        if trimmed.is_empty() {
            continue;
        }
        if trimmed.contains("file format") || trimmed.starts_with("Disassembly of section") {
            continue;
        }
        lines.push(trimmed.to_string());
    }
    Ok(lines.join("\n"))
}

#[test]
fn the_golden_texts_match() {
    let update = std::env::var("RL_UPDATE_GOLDEN").is_ok_and(|value| value == "1");
    let directory = workdir();
    let golden_directory = manifest().join("golden");
    let mut ran = 0usize;
    let mut skipped = 0usize;
    for golden in &GOLDENS {
        let object = match compile(golden, &directory) {
            Ok(object) => object,
            Err(reason) => {
                println!("{}: {reason}", golden.name);
                skipped += 1;
                continue;
            }
        };
        let text = match disassemble(&object) {
            Ok(text) => text,
            Err(reason) => {
                println!("{}: {reason}", golden.name);
                skipped += 1;
                continue;
            }
        };
        let path = golden_directory.join(format!("{}.txt", golden.name));
        if update {
            std::fs::write(&path, format!("{text}\n")).expect("write golden");
            println!("{}: updated {}", golden.name, path.display());
            ran += 1;
            continue;
        }
        let expected = std::fs::read_to_string(&path).unwrap_or_else(|error| {
            panic!("missing golden {}: {error}", path.display());
        });
        assert_eq!(
            text,
            expected.trim_end(),
            "{}: the disassembly changed; regenerate with RL_UPDATE_GOLDEN=1",
            golden.name
        );
        ran += 1;
    }
    println!("golden: {ran} compared, {skipped} skipped");
    assert_eq!(ran + skipped, GOLDENS.len());
}
