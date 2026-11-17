// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Stage the dynamic target for `include_bytes!`.
//!
//! `build.sh` compiles the PIE first and points `DYNAMIC_EXEC_IMAGE` at it. A plain workspace
//! build has no image; it still compiles, with an empty embedded image, so host checks do not
//! depend on a target build having run.

use std::env;
use std::fs;
use std::path::PathBuf;

fn main() {
    println!("cargo:rerun-if-env-changed=DYNAMIC_EXEC_IMAGE");
    let output = PathBuf::from(env::var("OUT_DIR").expect("OUT_DIR")).join("dynamic-exec.bin");
    let source = env::var("DYNAMIC_EXEC_IMAGE").unwrap_or_default();
    let image = if source.is_empty() {
        Vec::new()
    } else {
        fs::read(&source).unwrap_or_default()
    };
    fs::write(&output, image).expect("write embedded dynamic image");
    println!("cargo:rustc-env=DYNAMIC_EXEC_IMAGE={}", output.display());
}
