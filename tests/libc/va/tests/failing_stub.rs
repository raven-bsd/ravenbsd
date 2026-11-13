// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! The H-16 failing-stub test: on aarch64 and riscv64, until the reviewer sets
//! `cfg(raven_va_verified_aarch64)` / `cfg(raven_va_verified_riscv64)`, every non-`v` form is
//! a branch to a symbol that is never defined, so a program that references `printf` does not
//! LINK (and the linker names the missing symbol).
//!
//! The test compiles the same tiny instance the golden test uses and inspects it with
//! `llvm-nm`, then attempts a link with `rust-lld` and checks the diagnostic.

#![allow(unexpected_cfgs)] // `raven_va_softfloat` and `raven_va_verified_*` are build-profile cfgs

use std::path::{Path, PathBuf};
use std::process::Command;

fn manifest() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn workdir() -> PathBuf {
    if let Ok(directory) = std::env::var("CARGO_TARGET_TMPDIR") {
        return PathBuf::from(directory).join("va-failing");
    }
    manifest()
        .ancestors()
        .nth(3)
        .map(|root| root.join("target/va-failing"))
        .unwrap_or_else(|| manifest().join("target/va-failing"))
}

fn instance_source() -> String {
    let trampoline =
        manifest().join("../../../libc/ravenlibc/crates/raven-libc/src/va/trampoline.rs");
    format!(
        "#![no_std]\n\
         #[path = \"{}\"]\n\
         mod trampoline;\n\
         unsafe extern \"C\" {{\n    fn vdemo(a: usize, b: usize, c: usize, ap: *mut u8) -> i32;\n}}\n\
         crate::raven_variadic!(prefix: \"t_\", export: \"snprintf\", target: vdemo, named: 3);\n",
        trampoline.display()
    )
}

fn build(name: &str, target: &str, cfgs: &[&str]) -> Result<PathBuf, String> {
    let directory = workdir();
    std::fs::create_dir_all(&directory).map_err(|error| error.to_string())?;
    let source = directory.join(format!("{name}.rs"));
    std::fs::write(&source, instance_source()).map_err(|error| error.to_string())?;
    let object = directory.join(format!("{name}.o"));
    let mut command = Command::new("rustc");
    command
        .arg("--edition")
        .arg("2024")
        .arg("--target")
        .arg(target)
        .arg("--crate-type")
        .arg("lib")
        .arg("--emit")
        .arg("obj")
        .arg("-o")
        .arg(&object)
        .arg(&source);
    for cfg in cfgs {
        command.arg("--cfg").arg(cfg);
    }
    let output = command.output().map_err(|error| error.to_string())?;
    if !output.status.success() {
        let message = String::from_utf8_lossy(&output.stderr).into_owned();
        if message.contains("can't find crate for `core`")
            || message.contains("may not be installed")
        {
            return Err(format!("skipped: target {target} not installed"));
        }
        return Err(format!("rustc failed: {message}"));
    }
    Ok(object)
}

fn nm(arguments: &[&str], object: &Path) -> Result<String, String> {
    let output = Command::new("llvm-nm")
        .args(arguments)
        .arg(object)
        .output()
        .map_err(|error| error.to_string())?;
    if !output.status.success() {
        return Err(format!(
            "llvm-nm failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

fn rust_lld() -> Option<PathBuf> {
    let sysroot = Command::new("rustc")
        .arg("--print")
        .arg("sysroot")
        .output()
        .ok()?;
    if !sysroot.status.success() {
        return None;
    }
    let root = PathBuf::from(String::from_utf8_lossy(&sysroot.stdout).trim());
    let candidate = root.join("lib/rustlib/x86_64-unknown-linux-gnu/bin/rust-lld");
    candidate.is_file().then_some(candidate)
}

fn check_failing_stub(name: &str, target: &str) {
    let object = match build(name, target, &[]) {
        Ok(object) => object,
        Err(reason) => {
            println!("{name}: {reason}");
            return;
        }
    };
    let undefined = nm(&["-u"], &object).expect("llvm-nm -u");
    assert!(
        undefined.contains("__raven_variadic_unsupported_t_snprintf"),
        "{name}: the stub does not reference the undefined symbol: {undefined}"
    );
    let defined = nm(&["--defined-only"], &object).expect("llvm-nm");
    assert!(
        defined.contains(" t_snprintf"),
        "{name}: the stub symbol is not defined: {defined}"
    );
    // The link must fail, and it must fail naming the missing symbol.
    let Some(lld) = rust_lld() else {
        println!("{name}: rust-lld not found, link check skipped");
        return;
    };
    let executable = workdir().join(format!("{name}.elf"));
    let output = Command::new(lld)
        .arg("-flavor")
        .arg("gnu")
        .arg("-o")
        .arg(&executable)
        .arg(&object)
        .arg("-e")
        .arg("t_snprintf")
        .output()
        .expect("run rust-lld");
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
    assert!(
        !output.status.success(),
        "{name}: a program referencing the stub LINKED; the failing-stub rule is broken"
    );
    assert!(
        stderr.contains("__raven_variadic_unsupported_t_snprintf"),
        "{name}: the link error does not name the missing symbol: {stderr}"
    );
    println!("{name}: link fails as intended: {}", stderr.trim());
}

#[test]
fn aarch64_printf_does_not_link() {
    check_failing_stub("aarch64", "aarch64-unknown-none-softfloat");
}

#[test]
fn riscv64_printf_does_not_link() {
    check_failing_stub("riscv64", "riscv64gc-unknown-none-elf");
}

#[test]
fn the_verified_flag_turns_the_stub_into_a_trampoline() {
    // The reviewer's flag is the only thing that flips the arm; the test proves that the
    // flip produces a defined branch target rather than the undefined symbol.
    for (name, target, cfg) in [
        (
            "aarch64_verified",
            "aarch64-unknown-none-softfloat",
            "raven_va_verified_aarch64",
        ),
        (
            "riscv64_verified",
            "riscv64gc-unknown-none-elf",
            "raven_va_verified_riscv64",
        ),
    ] {
        let object = match build(name, target, &[cfg]) {
            Ok(object) => object,
            Err(reason) => {
                println!("{name}: {reason}");
                continue;
            }
        };
        let undefined = nm(&["-u"], &object).expect("llvm-nm -u");
        assert!(
            !undefined.contains("__raven_variadic_unsupported_"),
            "{name}: the verified arm still references the unsupported symbol: {undefined}"
        );
    }
}
