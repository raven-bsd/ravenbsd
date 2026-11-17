// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Failure and classification fixtures (section 5 step 7).

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use libc_world::classify::{StepLog, SymbolIndex, classify};
use libc_world::extract;
use libc_world::patches;

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

fn skip(tool: &str) -> bool {
    if Command::new(tool).arg("--version").output().is_ok() {
        return false;
    }
    eprintln!("SKIP: {tool} not found");
    true
}

fn nm() -> &'static str {
    if Command::new("llvm-nm").arg("--version").output().is_ok() {
        "llvm-nm"
    } else {
        "nm"
    }
}

#[test]
fn fx_hdr_is_a_missing_header() {
    if skip("clang") {
        return;
    }
    let temp = std::env::temp_dir().join(format!("libc-world-fx-hdr-{}", std::process::id()));
    let _ = fs::remove_dir_all(&temp);
    fs::create_dir_all(&temp).expect("temp");
    let source = fixtures().join("pkgs/fx-hdr/a.c");
    let output = Command::new("clang")
        .args([
            "--target=x86_64-unknown-ravenbsd",
            "-I",
            fixtures()
                .join("sysroot/usr/include")
                .to_str()
                .expect("path"),
            "-c",
            source.to_str().expect("path"),
            "-o",
            temp.join("a.o").to_str().expect("path"),
        ])
        .output()
        .expect("clang");
    assert!(!output.status.success());
    let log = StepLog {
        name: "make".to_string(),
        exit: Some(1),
        text: String::from_utf8_lossy(&output.stderr).into_owned(),
    };
    let classified = classify(&[log], &SymbolIndex::default(), &[]).expect("classified");
    assert_eq!(classified.class, "missing-header");
    assert_eq!(classified.detail, "raven/not_there.h");
    let _ = fs::remove_dir_all(&temp);
}

#[test]
fn fx_sym_yields_an_undefined_symbol() {
    if skip("clang") {
        return;
    }
    let temp = std::env::temp_dir().join(format!("libc-world-fx-sym-{}", std::process::id()));
    let _ = fs::remove_dir_all(&temp);
    fs::create_dir_all(&temp).expect("temp");
    let source = fixtures().join("pkgs/fx-sym/a.c");
    let object = temp.join("a.o");
    let status = Command::new("clang")
        .args([
            "--target=x86_64-unknown-ravenbsd",
            "-I",
            fixtures()
                .join("sysroot/usr/include")
                .to_str()
                .expect("path"),
            "-c",
            source.to_str().expect("path"),
            "-o",
            object.to_str().expect("path"),
        ])
        .status()
        .expect("clang");
    assert!(status.success());
    let extraction = extract::scan(
        nm(),
        std::slice::from_ref(&temp),
        "x86_64",
        &fixtures().join("sysroot"),
        &temp.join("log"),
    )
    .expect("scan");
    assert_eq!(extraction.undefined.get("rl_missing_function"), Some(&'U'));
    let _ = fs::remove_dir_all(&temp);
}

#[test]
fn fx_lib_is_a_missing_library() {
    let log = StepLog {
        name: "make".to_string(),
        exit: Some(1),
        text: "ld.lld: error: unable to find library -lrl_missing\n".to_string(),
    };
    let classified = classify(&[log], &SymbolIndex::default(), &[]).expect("classified");
    assert_eq!(classified.class, "missing-lib");
    assert_eq!(classified.detail, "rl_missing");
}

#[test]
fn fx_probe_reports_a_libc_probe_failure() {
    if skip("sh") {
        return;
    }
    let script = fixtures().join("pkgs/fx-probe/configure");
    let output = Command::new("sh").arg(&script).output().expect("sh");
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let index = SymbolIndex::from_names(&["strlcpy"], &["string.h"]);
    let log = StepLog {
        name: "configure".to_string(),
        exit: Some(1),
        text,
    };
    let classified = classify(&[log], &index, &[]).expect("classified");
    assert_eq!(classified.class, "missing-symbol");
    assert!(classified.libc_probe_failed.contains("strlcpy"));
}

#[test]
fn fx_patch_counts_as_a_libc_patch() {
    let path = fixtures().join("pkgs/fx-patch/0001-missing-header.patch");
    let text = fs::read_to_string(&path).expect("patch");
    let header = patches::parse_header(&text).expect("header");
    let applied = patches::classify("0001-missing-header.patch", header);
    let counts = patches::account(&[applied]);
    assert_eq!(counts.libc, 1);
}

#[test]
fn the_fake_sysroot_has_the_three_headers() {
    for header in ["stdio.h", "stdlib.h", "string.h"] {
        assert!(
            fixtures()
                .join("sysroot/usr/include")
                .join(header)
                .is_file(),
            "{header}"
        );
    }
}

#[test]
fn the_fixture_packages_are_single_top_directories() {
    for name in [
        "zlib-1.0",
        "lua-1.0",
        "sqlite-1.0",
        "pcre2-1.0",
        "toybox-1.0",
    ] {
        assert!(fixtures().join("pkgs").join(name).is_dir(), "{name}");
    }
}
