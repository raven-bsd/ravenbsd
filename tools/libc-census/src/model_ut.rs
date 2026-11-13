// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Tests for the shared data types.

use super::*;

#[test]
fn ring_serialises_numbers_and_private() {
    #[derive(serde::Serialize)]
    struct Wrapper {
        number: Ring,
        private: Ring,
    }
    let text = toml::to_string(&Wrapper {
        number: Ring::Number(2),
        private: Ring::private(),
    })
    .unwrap();
    assert!(text.contains("number = 2"));
    assert!(text.contains("private = \"private\""));
    assert!(Ring::Number(9).validate().is_err());
    assert!(Ring::Private("other".to_string()).validate().is_err());
}

#[test]
fn symbol_row_omits_empty_fields_and_keeps_contract_order() {
    let row = SymbolRow {
        name: "strlcpy".to_string(),
        ring: Some(Ring::Number(1)),
        header: Some("string.h".to_string()),
        kind: Some("func".to_string()),
        standard: vec!["posix-2024".to_string()],
        families: vec!["bsd".to_string(), "linux".to_string()],
        libcs: vec!["glibc".to_string()],
        n6: Some(1),
        ring_computed: 0,
        ring_source: "assignment".to_string(),
        ..SymbolRow::default()
    };
    let text = toml::to_string(&row).unwrap();
    let name = text.find("name =").unwrap();
    let ring = text.find("ring =").unwrap();
    let header = text.find("header =").unwrap();
    let kind = text.find("kind =").unwrap();
    let standard = text.find("standard =").unwrap();
    let families = text.find("families =").unwrap();
    let libcs = text.find("libcs =").unwrap();
    let computed = text.find("ring_computed =").unwrap();
    assert!(name < ring && ring < header && header < kind);
    assert!(kind < standard && standard < families && families < libcs);
    assert!(libcs < computed);
    assert!(!text.contains("personality"));
    assert!(!text.contains("ring_override"));
}

#[test]
fn kind_parses_the_contract_spellings() {
    assert_eq!(Kind::parse("func").unwrap(), Kind::Func);
    assert_eq!(Kind::parse("data").unwrap(), Kind::Data);
    assert_eq!(Kind::parse("macro").unwrap(), Kind::Macro);
    assert_eq!(Kind::parse("header-only").unwrap(), Kind::HeaderOnly);
    assert!(Kind::parse("object").is_err());
}

#[test]
fn cell_total_saturates_and_text_is_doc15_shape() {
    let cell = Cell { d: 3, h: 2 };
    assert_eq!(cell.total(), 5);
    assert_eq!(cell.text(), "3/2");
    assert_eq!(Cell::default().text(), "0/0");
}

#[test]
fn donor_families_match_doc02() {
    assert_eq!(donor_family("glibc"), Some("linux"));
    assert_eq!(donor_family("bionic"), Some("linux"));
    assert_eq!(donor_family("netbsd"), Some("bsd"));
    assert_eq!(donor_family("illumos"), Some("solaris"));
    assert_eq!(donor_family("mlibc"), None);
    assert_eq!(donor_family("picolibc"), None);
}

#[test]
fn exit_codes_are_the_cli_contract() {
    assert_eq!(Exit::Ok.code(), 0);
    assert_eq!(Exit::GateFailed.code(), 1);
    assert_eq!(Exit::Usage.code(), 2);
}
