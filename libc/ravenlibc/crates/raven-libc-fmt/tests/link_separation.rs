// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! R-13 link-separation check: the integer-only instantiation must not drag any
//! numeric symbol into the archive member.
//!
//! The test locates the crate's rlib, runs `llvm-nm` or `nm` on it, and fails on
//! a symbol containing `dtoa`, `strtod`, `core3fmt` or one of the soft-float
//! helpers. Set `RL_NO_NM_OK=1` on a machine without a symbol lister; the report
//! then says so. `RAVEN_FMT_RLIB` can point at the `*-unknown-none` rlib built
//! by the acceptance commands.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects
)]

use std::path::{Path, PathBuf};
use std::process::Command;

fn find_rlib() -> Option<PathBuf> {
    if let Ok(explicit) = std::env::var("RAVEN_FMT_RLIB") {
        let p = PathBuf::from(explicit);
        if p.is_file() {
            return Some(p);
        }
    }
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    let target = std::env::var_os("CARGO_TARGET_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| manifest.join("../../../../target"));
    for profile in ["release", "debug"] {
        let deps = target.join(profile).join("deps");
        let Ok(entries) = std::fs::read_dir(&deps) else {
            continue;
        };
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            if name.starts_with("libraven_libc_fmt-") && name.ends_with(".rlib") {
                return Some(entry.path());
            }
        }
        let direct = target.join(profile).join("libraven_libc_fmt.rlib");
        if direct.is_file() {
            return Some(direct);
        }
    }
    None
}

fn nm_output(path: &Path) -> Option<String> {
    for tool in ["llvm-nm", "nm"] {
        let out = Command::new(tool).arg("-A").arg(path).output();
        if let Ok(out) = out
            && out.status.success()
        {
            return Some(String::from_utf8_lossy(&out.stdout).into_owned());
        }
    }
    None
}

#[test]
fn integer_instantiations_link_no_float_code() {
    let Some(rlib) = find_rlib() else {
        if std::env::var_os("RL_NO_NM_OK").is_some() {
            eprintln!("link-separation: no rlib found; RL_NO_NM_OK=1, skipped");
            return;
        }
        panic!("no raven_libc_fmt rlib found; set RAVEN_FMT_RLIB or build first");
    };
    let Some(text) = nm_output(&rlib) else {
        if std::env::var_os("RL_NO_NM_OK").is_some() {
            eprintln!("link-separation: no nm/llvm-nm; RL_NO_NM_OK=1, skipped");
            return;
        }
        panic!("no nm or llvm-nm; set RL_NO_NM_OK=1 to record the skip");
    };
    let forbidden = [
        "dtoa", "strtod", "core3fmt", "__adddf3", "__subdf3", "__muldf3", "__divdf3", "__addsf3",
        "__mulsf3", "__divsf3",
    ];
    let mut bad = Vec::new();
    for line in text.lines() {
        for needle in forbidden {
            if line.contains(needle) {
                bad.push(format!("{needle}: {line}"));
            }
        }
    }
    assert!(
        bad.is_empty(),
        "forbidden numeric symbols in {}:\n{}",
        rlib.display(),
        bad.join("\n")
    );
    println!(
        "link-separation: {} clean ({} lines)",
        rlib.display(),
        text.lines().count()
    );
}
