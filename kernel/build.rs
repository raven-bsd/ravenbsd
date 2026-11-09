// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Build script for the kernel: selects the linker script, fixes the page size and passes the
//! build id through to the panic banner (S04). It also checks that the link address hard-coded in
//! the linker script matches the `raven-abi` memory layout contract.

use std::env;
use std::path::PathBuf;

/// Higher-half virtual base of the kernel image written into `linker/x86_64.ld`.
const X86_64_LINK_BASE: u64 = 0xFFFF_FFFF_8020_0000;
/// Offset from `raven_abi::layout::x86_64::KERNEL_IMAGE_BASE` to the link base above.
const X86_64_IMAGE_OFFSET: u64 = 0x20_0000;

fn main() {
    let arch = env::var("CARGO_CFG_TARGET_ARCH").expect("CARGO_CFG_TARGET_ARCH must be set");
    let manifest_dir =
        PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR must be set"));

    let linker_script = match arch.as_str() {
        "x86_64" => "linker/x86_64.ld",
        other => panic!("raven-kernel: no linker script for target architecture '{other}'"),
    };

    println!(
        "cargo:rustc-link-arg-bins=-T{}",
        manifest_dir.join(linker_script).display()
    );
    println!("cargo:rerun-if-changed={linker_script}");
    println!("cargo:rustc-link-arg-bins=-zmax-page-size=4096");
    println!("cargo:rustc-link-arg-bins=--gc-sections");
    // The workspace release profile strips all symbols, but the S01 acceptance checks `nm` and
    // later kernel backtraces (S04+) need the symbol table. This last link argument downgrades the
    // strip to debug information only.
    println!("cargo:rustc-link-arg-bins=--strip-debug");

    let build_id = env::var("RAVEN_BUILD_ID").unwrap_or_else(|_| String::from("dev"));
    println!("cargo:rustc-env=RAVEN_BUILD_ID={build_id}");
    println!("cargo:rerun-if-env-changed=RAVEN_BUILD_ID");

    let expected = raven_abi::layout::x86_64::KERNEL_IMAGE_BASE + X86_64_IMAGE_OFFSET;
    if X86_64_LINK_BASE != expected {
        panic!(
            "raven-kernel: linker script base {X86_64_LINK_BASE:#018x} does not match \
             raven-abi KERNEL_IMAGE_BASE + {X86_64_IMAGE_OFFSET:#x} = {expected:#018x}"
        );
    }
}
