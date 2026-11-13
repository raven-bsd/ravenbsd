// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! The "no forbidden constructs" test of C59 section 7.2: the variadic module must not use
//! `f32`/`f64` values, `core::fmt`, `format_args`, `alloc`, `dyn`, the nightly `c_variadic`
//! gate or any `feature` attribute.
//!
//! The needles are assembled at run time so this file itself never contains the forbidden
//! attribute text the repository guard scans for.

#![allow(unexpected_cfgs)] // `raven_va_softfloat` and `raven_va_verified_*` are build-profile cfgs

use std::path::{Path, PathBuf};

fn source_files() -> Vec<PathBuf> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../../libc/ravenlibc/crates/raven-libc/src/va");
    let mut files = Vec::new();
    for entry in std::fs::read_dir(&root).expect("va directory") {
        let path = entry.expect("entry").path();
        if path.extension().is_some_and(|extension| extension == "rs") {
            files.push(path);
        }
    }
    files.sort();
    files
}

#[test]
fn the_module_has_no_forbidden_constructs() {
    let needles: [&str; 10] = [
        "f32",
        "f64",
        "core::fmt",
        "format_args",
        "alloc::",
        "extern crate alloc",
        "dyn ",
        "c_variadic",
        concat!("#![fea", "ture("),
        concat!("#[fea", "ture("),
    ];
    let mut checked = 0usize;
    for file in source_files() {
        let name = file
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned();
        if name.ends_with("_ut.rs") {
            // Unit tests are compiled with `std`; the rule is about the shipped module.
            continue;
        }
        let text = std::fs::read_to_string(&file).expect("read");
        // Comments are allowed to *name* the constructs (the README and the doc comments
        // explain what is forbidden); scan the code lines only.
        let mut in_block_comment = false;
        for (number, line) in text.lines().enumerate() {
            let trimmed = line.trim_start();
            if in_block_comment {
                if trimmed.contains("*/") {
                    in_block_comment = false;
                }
                continue;
            }
            if trimmed.starts_with("/*") {
                in_block_comment = !trimmed.contains("*/");
                continue;
            }
            if trimmed.starts_with("//") {
                continue;
            }
            for needle in &needles {
                if *needle == "f64" {
                    // The C43 trait method is `next_f64_bits`; the name is the contract, the
                    // type is not used. Everything else that says `f64` is forbidden.
                    let without_method = line.replace("next_f64_bits", "");
                    assert!(
                        !without_method.contains("f64"),
                        "{}:{}: forbidden construct `f64`",
                        file.display(),
                        number + 1
                    );
                    continue;
                }
                assert!(
                    !line.contains(*needle),
                    "{}:{}: forbidden construct `{needle}`",
                    file.display(),
                    number + 1
                );
            }
        }
        checked += 1;
    }
    assert!(checked >= 7, "only {checked} module files checked");
}

#[test]
fn the_module_files_are_the_contract() {
    let names: Vec<String> = source_files()
        .iter()
        .map(|path| {
            path.file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned()
        })
        .collect();
    for expected in [
        "mod.rs",
        "sysv_x86_64.rs",
        "aapcs64.rs",
        "riscv64.rs",
        "trampoline.rs",
        "table.rs",
        "env.rs",
        "mod_ut.rs",
        "sysv_x86_64_ut.rs",
        "aapcs64_ut.rs",
        "riscv64_ut.rs",
        "trampoline_ut.rs",
        "table_ut.rs",
        "env_ut.rs",
    ] {
        assert!(
            names.iter().any(|name| name == expected),
            "missing module file {expected}"
        );
    }
}
