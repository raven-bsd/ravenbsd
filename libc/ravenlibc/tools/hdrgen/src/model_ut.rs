// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Unit tests for the TOML model.

use super::*;

#[test]
fn types_file_parses_a_minimal_row() {
    let text = r#"
schema = 1
[[type]]
name = "size_t"
rust = "usize"
c = "unsigned long"
bits = 64
signed = false
"#;
    let parsed: TypesFile = toml::from_str(text).expect("types");
    assert_eq!(parsed.schema, 1);
    assert_eq!(parsed.types.len(), 1);
    assert_eq!(parsed.types[0].name, "size_t");
    assert_eq!(parsed.types[0].bits, 64);
}

#[test]
fn types_file_rejects_unknown_keys() {
    let text = r#"
schema = 1
[[type]]
name = "x"
rust = "u8"
c = "unsigned char"
bits = 8
signed = false
surprise = true
"#;
    assert!(toml::from_str::<TypesFile>(text).is_err());
}

#[test]
fn structs_file_parses_records_and_opaque_types() {
    let text = r#"
schema = 1
[[record]]
name = "point"
rust = "Point"
c = "struct point"
size = 8
align = 4
fields = [ { name = "x", ty = "i32", off = 0 }, { name = "y", ty = "i32", off = 4 } ]
[[opaque]]
name = "blob"
rust = "Blob"
c = "blob_t"
size = 16
align = 8
"#;
    let parsed: StructsFile = toml::from_str(text).expect("structs");
    assert_eq!(parsed.record.len(), 1);
    assert_eq!(parsed.record[0].fields.len(), 2);
    assert_eq!(parsed.opaque.len(), 1);
    assert!(parsed.opaque[0].musl.is_none());
}

#[test]
fn const_file_defaults_and_forms() {
    let text = r#"
schema = 1
group = "demo"
c_header = "bits/abi_demo.h"
guard = "_D"
consts = [
  { name = "A", value = 1 },
  { name = "B", alias = "A" },
  { name = "C", or = [1, "A"] },
  { name = "D", shl = [2, 3] },
]
"#;
    let parsed: ConstFile = toml::from_str(text).expect("consts");
    assert_eq!(parsed.rust_type, "i32");
    assert_eq!(parsed.base, "dec");
    assert!(parsed.source.is_empty());
    assert_eq!(parsed.consts.len(), 4);
    assert!(matches!(
        parsed.consts[2].or.as_deref(),
        Some([Term::Int(1), Term::Name(_)])
    ));
    assert_eq!(parsed.consts[3].shl, Some([2, 3]));
}

#[test]
fn const_file_rejects_function_like_keys() {
    let text = r#"
schema = 1
group = "demo"
c_header = "bits/abi_demo.h"
guard = "_D"
consts = [ { name = "A", value = 1, function = true } ]
"#;
    assert!(toml::from_str::<ConstFile>(text).is_err());
}

#[test]
fn int_or_table_reads_both_shapes() {
    let shared = IntOrTable::Int(8);
    assert_eq!(shared.get("x86_64"), Some(8));
    assert!(shared.is_shared());
    let table = IntOrTable::Table(BTreeMap::from([
        ("x86_64".to_string(), 1),
        ("aarch64".to_string(), 2),
    ]));
    assert_eq!(table.get("aarch64"), Some(2));
    assert_eq!(table.get("riscv64"), None);
    assert!(!table.is_shared());
    assert_eq!(table.arches().len(), 2);
    let equal = IntOrTable::Table(BTreeMap::from([
        ("x86_64".to_string(), 4),
        ("aarch64".to_string(), 4),
    ]));
    assert!(equal.is_shared());
}

#[test]
fn deviation_values_are_per_arch_with_ours_as_the_default() {
    let row: Deviation = toml::from_str(
        r#"
name = "x"
against = "musl"
ours = 7
theirs = { x86_64 = 9 }
reason = "r"
"#,
    )
    .expect("deviation");
    assert_eq!(row.ours_int("x86_64"), Some(7));
    assert_eq!(row.theirs_int("x86_64"), Some(9));
    assert_eq!(row.theirs_int("aarch64"), Some(7));
}

#[test]
fn layout_deviation_values_parse_size_and_align() {
    let row: Deviation = toml::from_str(
        r#"
name = "stat"
against = "musl"
ours = "128/8"
theirs = { x86_64 = "144/8" }
reason = "r"
"#,
    )
    .expect("deviation");
    assert_eq!(row.ours_layout("x86_64"), Some((128, 8)));
    assert_eq!(row.theirs_layout("x86_64"), Some((144, 8)));
    assert_eq!(row.theirs_layout("aarch64"), Some((128, 8)));
}

#[test]
fn describe_value_covers_the_three_shapes() {
    assert_eq!(describe_value(&toml::Value::Integer(3)), "3");
    assert_eq!(describe_value(&toml::Value::String("x".into())), "\"x\"");
    assert!(describe_value(&toml::Value::Table(toml::map::Map::new())).contains('{'));
}
