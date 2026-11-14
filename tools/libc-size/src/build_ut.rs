// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Unit tests for the build step descriptions.

use super::*;

#[test]
fn command_spec_keeps_an_argument_vector() {
    let spec = CommandSpec::new(
        Path::new("/tool/rustc"),
        vec!["-C".to_string(), "opt-level=z".to_string()],
        PathBuf::from("/repo"),
    );
    assert_eq!(spec.program(), Path::new("/tool/rustc"));
    assert_eq!(spec.args().len(), 2);
    assert_eq!(spec.cwd(), Path::new("/repo"));
    assert_eq!(spec.display_line(), "/tool/rustc -C opt-level=z");
}

#[test]
fn display_quotes_only_arguments_with_spaces() {
    let spec = CommandSpec::new(
        Path::new("tool"),
        vec!["plain".to_string(), "with space".to_string()],
        PathBuf::from("."),
    );
    assert_eq!(spec.display_line(), "tool plain \"with space\"");
}

#[test]
fn compact_error_prefers_the_first_real_error_line() {
    let stderr = "warning: x\nerror: first\nerror: second\nnote: y\n";
    assert_eq!(compact_error(stderr), "error: first");
}

#[test]
fn compact_error_prefers_an_undefined_symbol_line() {
    let stderr = "error: linking with `rust-lld` failed\n  = note: rust-lld: error: undefined symbol: fgets\nerror: aborting due to 1 previous error\n";
    assert_eq!(
        compact_error(stderr),
        "= note: rust-lld: error: undefined symbol: fgets"
    );
}

#[test]
fn compact_error_falls_back_to_the_last_line() {
    assert_eq!(
        compact_error("note: a\n\n  plain failure  \n"),
        "plain failure"
    );
    assert_eq!(compact_error(""), "tool failed");
}

#[test]
fn static_products_need_both_files() {
    let dir = crate::testutil::TempDir::new("build-products").expect("temp");
    assert!(!static_products(dir.path()));
    dir.write("libc.a", b"!<arch>\n").expect("archive");
    assert!(!static_products(dir.path()));
    dir.write("crt1.o", b"obj").expect("crt");
    assert!(static_products(dir.path()));
}

#[test]
fn dynamic_product_is_the_exact_c62_path() {
    assert_eq!(
        dynamic_product(Path::new("/repo/build/x86_64")),
        PathBuf::from("/repo/build/x86_64/libc.so.1")
    );
}

#[test]
fn libc_pack_command_carries_arch_and_set() {
    let spec = libc_pack_command(
        Path::new("/cargo"),
        Path::new("/repo"),
        Arch::Riscv64,
        Set::Server,
    );
    assert_eq!(
        spec.args(),
        &[
            "xtask".to_string(),
            "libc-pack".to_string(),
            "--arch".to_string(),
            "riscv64".to_string(),
            "--set".to_string(),
            "server".to_string(),
        ]
    );
}

#[test]
fn output_directories_follow_the_protocol() {
    let root = Path::new("/repo");
    let out = root.join("build/size/x86_64/tiny/static/rust");
    let spec = crate::probe::rust_static_command(
        Path::new("/rustc"),
        root,
        Arch::X86_64,
        Set::Tiny,
        "P2",
        Path::new("/repo/build/x86_64"),
        &out,
        &[],
    );
    assert!(spec.args().iter().any(|arg| arg.ends_with("p2.elf")));
    assert!(spec.args().iter().any(|arg| arg.ends_with("p2.map")));
}
