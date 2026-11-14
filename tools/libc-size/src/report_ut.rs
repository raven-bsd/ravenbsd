// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Unit tests for the result shapes and the deterministic report writer.

use super::*;

fn sample_results() -> Results {
    let mut results = Results::new(Toolchain {
        rustc: "rustc 1.97.1".to_string(),
        lld: "LLD 22.1.6".to_string(),
    });
    results.rows.push(Row::measured(
        "P2",
        Arch::X86_64,
        Linkage::Static,
        Set::Tiny,
        Lang::Rust,
        66417,
        6936,
        1051776,
        80,
        0,
        0,
        0,
        0,
    ));
    results.rows.push(Row::measured(
        "P0",
        Arch::X86_64,
        Linkage::Static,
        Set::Tiny,
        Lang::Rust,
        27248,
        6384,
        990952,
        37,
        0,
        0,
        0,
        0,
    ));
    results.rows.push(Row::skipped(
        "P0",
        Arch::X86_64,
        Linkage::Dynamic,
        Set::Server,
        Lang::Rust,
        "libc.so.1 absent (C62 not landed)",
    ));
    results.finalize();
    results
}

#[test]
fn rows_sort_probe_then_matrix() {
    let results = sample_results();
    let ids: Vec<&str> = results.rows.iter().map(|row| row.probe.as_str()).collect();
    assert_eq!(ids, vec!["P0", "P2", "P0"]);
    assert_eq!(
        results.rows.first().expect("first").linkage,
        Linkage::Static
    );
    assert_eq!(results.rows.get(2).expect("last").linkage, Linkage::Dynamic);
}

#[test]
fn generated_is_null_and_serialized() {
    let results = sample_results();
    let json = results.to_json().expect("json");
    assert!(json.contains("\"generated\": null"));
    assert!(!json.contains("20"), "no timestamp");
    assert!(json.contains("\"S\": 66417"));
    assert!(json.contains("\"B\": 6936"));
}

#[test]
fn two_serializations_are_byte_identical() {
    let results = sample_results();
    assert_eq!(results.to_json().expect("a"), results.to_json().expect("b"));
}

#[test]
fn result_round_trips_through_a_file() {
    let dir = crate::testutil::TempDir::new("report-roundtrip").expect("temp");
    let path = dir.path().join("libc-size.json");
    let results = sample_results();
    results.write(&path).expect("write");
    let loaded = Results::read(&path).expect("read");
    assert_eq!(loaded, results);
}

#[test]
fn skipped_rows_are_not_measured() {
    let row = Row::skipped(
        "P0",
        Arch::X86_64,
        Linkage::Dynamic,
        Set::Server,
        Lang::Rust,
        "libc.so.1 absent (C62 not landed)",
    );
    assert!(!row.is_measured());
    assert_eq!(row.status, "skipped: libc.so.1 absent (C62 not landed)");
}

#[test]
fn row_lookup_is_by_matrix_cell() {
    let results = sample_results();
    let row = results
        .row("P2", Arch::X86_64, Linkage::Static, Set::Tiny, Lang::Rust)
        .expect("P2");
    assert_eq!(row.s, 66417);
    assert!(
        results
            .row("P2", Arch::Aarch64, Linkage::Static, Set::Tiny, Lang::Rust)
            .is_none()
    );
}

#[test]
fn markdown_has_one_table_per_arch_and_skips() {
    let results = sample_results();
    let markdown = render_markdown(&results);
    assert!(markdown.contains("## x86_64"));
    assert!(markdown.contains("| P2 | static | tiny | rust | 66417 |"));
    assert!(markdown.contains("skipped: libc.so.1 absent (C62 not landed)"));
    assert!(!markdown.contains("## aarch64"));
}

#[test]
fn probe_numbers_sort_numerically() {
    assert_eq!(probe_number("P0"), 0);
    assert_eq!(probe_number("P9"), 9);
    assert!(probe_number("P9") < probe_number("P10"));
    assert_eq!(probe_number("X"), u8::MAX);
}

#[test]
fn set_symbol_rows_and_boot_rows_sort() {
    let mut results = Results::new(Toolchain::default());
    results.set_symbols.push(SetSymbolRow {
        arch: Arch::Riscv64,
        tiny_only: vec![],
        server_only: vec![],
        status: "ok".to_string(),
    });
    results.set_symbols.push(SetSymbolRow {
        arch: Arch::X86_64,
        tiny_only: vec![],
        server_only: vec![],
        status: "ok".to_string(),
    });
    results.boot.push(BootRow {
        arch: Arch::Riscv64,
        syscalls_before_main: 1,
        start_instructions: 200,
        init_array: 0,
        preinit_array: 0,
        ctors: 0,
        early_malloc: Vec::new(),
        forbidden_refs: Vec::new(),
        rodata: Vec::new(),
        notes: Vec::new(),
        status: "ok".to_string(),
    });
    results.boot.push(BootRow {
        arch: Arch::X86_64,
        syscalls_before_main: 1,
        start_instructions: 190,
        init_array: 0,
        preinit_array: 0,
        ctors: 0,
        early_malloc: Vec::new(),
        forbidden_refs: Vec::new(),
        rodata: Vec::new(),
        notes: Vec::new(),
        status: "ok".to_string(),
    });
    results.finalize();
    assert_eq!(results.boot.first().expect("first").arch, Arch::X86_64);
    assert_eq!(
        results.set_symbols.first().expect("first").arch,
        Arch::X86_64
    );
}

#[test]
fn libc_so_row_uses_capital_metric_keys() {
    let row = LibcSoRow {
        arch: Arch::X86_64,
        path: "build/x86_64/libc.so.1".to_string(),
        s: 1,
        bss: 2,
        file: 3,
        needed: 1,
        jump_slot: 4,
        glob_dat: 5,
        relative: 6,
        data_rel_ro: 7,
        status: "ok".to_string(),
    };
    let json = serde_json::to_string(&row).expect("json");
    assert!(json.contains("\"S\":1"));
    assert!(json.contains("\"B\":2"));
    assert!(json.contains("\"F\":3"));
}
