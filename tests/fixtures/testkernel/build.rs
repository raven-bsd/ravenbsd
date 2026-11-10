// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Build script for the loader test kernel: select the linker script and page size.

use std::env;
use std::path::PathBuf;

fn main() {
    let manifest_dir =
        PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR must be set"));
    let arch = env::var("CARGO_CFG_TARGET_ARCH").unwrap_or_default();
    let script_name = if arch == "aarch64" {
        "linker-aarch64.ld"
    } else {
        "linker.ld"
    };
    let script = manifest_dir.join(script_name);

    println!("cargo:rustc-link-arg-bins=-T{}", script.display());
    println!("cargo:rerun-if-changed={script_name}");
    println!("cargo:rustc-link-arg-bins=-zmax-page-size=4096");
    println!("cargo:rustc-link-arg-bins=--gc-sections");
}
