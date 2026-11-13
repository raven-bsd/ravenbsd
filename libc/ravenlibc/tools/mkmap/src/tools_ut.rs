// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Tests for the `nm`/`readelf` parsers.

use super::*;

#[test]
fn parses_gnu_nm_archive_output() {
    let symbols = parse_nm("libx.a[m.o]:\ndata D 0 4\nfoo T 0 f\n\nlibx.a[n.o]:\nbar T 0 10\n");
    assert_eq!(symbols.len(), 3);
    assert_eq!(symbols.first().map(|s| s.member.as_str()), Some("m.o"));
    assert_eq!(symbols.first().map(|s| s.name.as_str()), Some("data"));
    assert_eq!(symbols.first().map(|s| s.size), Some(4));
    assert!(symbols.get(1).is_some_and(ArchiveSymbol::is_text));
    assert!(symbols.first().is_some_and(ArchiveSymbol::is_data));
}

#[test]
fn parses_llvm_nm_archive_output() {
    let symbols = parse_nm("\nm.o:\nfoo T 0 3\n");
    assert_eq!(symbols.len(), 1);
    assert_eq!(symbols.first().map(|s| s.member.as_str()), Some("m.o"));
}

#[test]
fn a_weak_definition_is_recognised() {
    let symbols = parse_nm("a.a[m.o]:\nhook W 0 4\n");
    assert!(symbols.first().is_some_and(ArchiveSymbol::is_weak));
}

#[test]
fn parses_versioned_dynamic_symbols() {
    let symbols = parse_dyn_syms(
        "Num: Value Size Type Bind Vis Ndx Name\n\
         1: 0000000000001000 10 FUNC GLOBAL DEFAULT 12 strlen@@RAVEN_1.0\n\
         2: 0000000000001010 8 OBJECT GLOBAL DEFAULT 12 __rl_x@@RAVENprivate_1.0\n\
         3: 0000000000001020 0 NOTYPE GLOBAL DEFAULT UND absent\n",
    );
    assert_eq!(symbols.len(), 2);
    assert_eq!(
        symbols.first().map(|s| s.version.as_str()),
        Some("RAVEN_1.0")
    );
    assert_eq!(symbols.get(1).map(|s| s.name.as_str()), Some("__rl_x"));
}

#[test]
fn an_unversioned_dynamic_symbol_has_an_empty_version() {
    let symbols = parse_dyn_syms(
        "Num: Value Size Type Bind Vis Ndx Name\n\
         1: 0000000000001000 10 FUNC GLOBAL DEFAULT 12 strlen\n",
    );
    assert_eq!(symbols.first().map(|s| s.version.as_str()), Some(""));
}

#[test]
fn counts_jump_slots_only() {
    let text = "Relocation section '.rela.plt':\n\
                0000000000001000 R_X86_64_JUMP_SLOT 0000000000000000 strlen + 0\n\
                0000000000001008 R_X86_64_GLOB_DAT 0000000000000000 environ + 0\n\
                0000000000001010 R_X86_64_JUMP_SLOT 0000000000000000 memcpy + 0\n";
    assert_eq!(count_jump_slots(text), 2);
}
