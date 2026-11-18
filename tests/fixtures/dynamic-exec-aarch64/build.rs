// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Stage the aarch64 dynamic target for `include_bytes!`.

use std::env;
use std::fs;
use std::path::PathBuf;

fn main() {
    println!("cargo:rerun-if-env-changed=DYNAMIC_EXEC_IMAGE");
    let output = PathBuf::from(env::var("OUT_DIR").expect("OUT_DIR")).join("dynamic-exec.bin");
    let source = env::var("DYNAMIC_EXEC_IMAGE").unwrap_or_default();
    if !source.is_empty() {
        // The path is stable across qtest runs while the C image content changes. Without this
        // dependency Cargo keeps the first embedded image forever and the launcher execs a stale
        // PIE, which page-faults at the loader base (0x1000).
        println!("cargo:rerun-if-changed={source}");
    }
    let image = if source.is_empty() {
        Vec::new()
    } else {
        fs::read(&source).unwrap_or_default()
    };
    fs::write(&output, image).expect("write embedded dynamic image");
    println!("cargo:rustc-env=DYNAMIC_EXEC_IMAGE={}", output.display());
}
