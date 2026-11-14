// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Unit tests for the `libc.so.1` row and the C62 skip discipline.

use super::*;

#[test]
fn skipped_row_has_the_exact_reason_and_path() {
    let row = skipped_row(Arch::X86_64);
    assert_eq!(row.status, SKIPPED_ABSENT);
    assert_eq!(
        skipped_row(Arch::X86_64).status,
        format!("skipped: {}", absence_reason())
    );
    assert_eq!(row.path, "build/x86_64/libc.so.1");
    assert_eq!(row.s, 0);
}

#[test]
fn every_arch_has_its_exact_path() {
    assert_eq!(
        so_path(Arch::Aarch64).display().to_string(),
        "build/aarch64/libc.so.1"
    );
    assert_eq!(
        so_path(Arch::Riscv64).display().to_string(),
        "build/riscv64/libc.so.1"
    );
}

#[test]
fn present_checks_the_exact_file() {
    let dir = crate::testutil::TempDir::new("relocs-present").expect("temp");
    assert!(!present(dir.path()));
    dir.write("libc.so.1", b"elf").expect("write");
    assert!(present(dir.path()));
}

#[test]
fn render_names_every_relocation_class() {
    let row = LibcSoRow {
        arch: Arch::X86_64,
        path: "build/x86_64/libc.so.1".to_string(),
        s: 100,
        bss: 1,
        file: 200,
        needed: 1,
        jump_slot: 2,
        glob_dat: 3,
        relative: 4,
        data_rel_ro: 5,
        status: "ok".to_string(),
    };
    let text = render(&row);
    assert!(text.contains("jump_slot=2"));
    assert!(text.contains("relative=4"));
    assert!(text.contains("data.rel.ro=5"));
}

#[test]
fn measure_returns_the_skip_row_when_absent() {
    let tools = crate::tools::Tools {
        size: std::path::PathBuf::from("/nonexistent/size"),
        nm: std::path::PathBuf::from("/nonexistent/nm"),
        readobj: std::path::PathBuf::from("/nonexistent/readobj"),
        objdump: std::path::PathBuf::from("/nonexistent/objdump"),
        strip: std::path::PathBuf::from("/nonexistent/strip"),
        ar: std::path::PathBuf::from("/nonexistent/ar"),
        objcopy: std::path::PathBuf::from("/nonexistent/objcopy"),
        lld: std::path::PathBuf::from("/nonexistent/lld"),
        rustc: std::path::PathBuf::from("/nonexistent/rustc"),
        cargo: std::path::PathBuf::from("/nonexistent/cargo"),
        clang: None,
        rustc_version: String::new(),
        lld_version: String::new(),
    };
    let dir = crate::testutil::TempDir::new("relocs-measure").expect("temp");
    let row = measure(
        &tools,
        dir.path(),
        Arch::X86_64,
        dir.path(),
        "build/x86_64/libc.so.1",
    )
    .expect("skip");
    assert_eq!(row.status, SKIPPED_ABSENT);
}
