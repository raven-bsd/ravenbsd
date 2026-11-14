// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Unit tests for the crate-level types and the headroom arithmetic.

use super::*;

#[test]
fn headroom_table_of_the_ticket() {
    assert_eq!(with_headroom(0, 3), 0);
    assert_eq!(with_headroom(1, 3), 2);
    assert_eq!(with_headroom(33, 3), 34);
    assert_eq!(with_headroom(100, 3), 103);
    assert_eq!(with_headroom(1787, 3), 1841);
    assert_eq!(with_headroom(18169, 3), 18715);
}

#[test]
fn headroom_is_strictly_ceil() {
    // 100 * 1.03 = 103 exactly; one more byte must round up.
    assert_eq!(with_headroom(101, 3), 105);
    assert_eq!(with_headroom(103, 0), 103);
    assert_eq!(with_headroom(104, 0), 104);
}

#[test]
fn headroom_does_not_overflow() {
    assert_eq!(with_headroom(u64::MAX, 3), u64::MAX);
}

#[test]
fn arch_names_and_triples() {
    assert_eq!(Arch::X86_64.short(), "x86_64");
    assert_eq!(Arch::Aarch64.triple(), "aarch64-unknown-none-softfloat");
    assert_eq!(Arch::Riscv64.triple(), "riscv64gc-unknown-none-elf");
    assert_eq!(Arch::X86_64.c_triple(), "x86_64-unknown-none-elf");
}

#[test]
fn arch_parse_all_is_sorted() {
    let arches = Arch::parse_list("all").expect("all");
    assert_eq!(arches, vec![Arch::X86_64, Arch::Aarch64, Arch::Riscv64]);
    assert!(Arch::parse_list("mips").is_err());
    assert!(Arch::parse_list("").is_err());
}

#[test]
fn arch_parse_list_deduplicates() {
    let arches = Arch::parse_list("riscv64,x86_64,riscv64").expect("list");
    assert_eq!(arches, vec![Arch::X86_64, Arch::Riscv64]);
}

#[test]
fn linkage_parse() {
    assert_eq!(
        Linkage::parse_list("static,dynamic").expect("both"),
        vec![Linkage::Static, Linkage::Dynamic]
    );
    assert_eq!(Linkage::parse_list("all").expect("all").len(), 2);
    assert!(Linkage::parse_list("lazy").is_err());
    assert_eq!(Linkage::Static.as_str(), "static");
}

#[test]
fn set_parse() {
    assert_eq!(
        Set::parse_list("tiny,server").expect("both"),
        vec![Set::Tiny, Set::Server]
    );
    assert_eq!(Set::Tiny.as_str(), "tiny");
    assert!(Set::parse_list("base").is_err());
}

#[test]
fn lang_parse() {
    assert_eq!(
        Lang::parse_list("rust,c").expect("both"),
        vec![Lang::Rust, Lang::C]
    );
    assert_eq!(Lang::C.as_str(), "c");
    assert!(Lang::parse_list("go").is_err());
}

#[test]
fn result_row_spells_snake_case() {
    let row = report::Row::skipped(
        "P0",
        Arch::X86_64,
        Linkage::Static,
        Set::Tiny,
        Lang::Rust,
        "x",
    );
    let json = serde_json::to_string(&row).expect("json");
    assert!(json.contains("\"linkage\":\"static\""));
    assert!(json.contains("\"lang\":\"rust\""));
}
