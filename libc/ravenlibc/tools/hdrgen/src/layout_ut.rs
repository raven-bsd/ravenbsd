// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Unit tests for the repr(C) layout arithmetic.

use super::*;
use crate::model::StructsFile;

fn structs(text: &str) -> StructsFile {
    toml::from_str(text).expect("structs")
}

#[test]
fn parses_every_field_type() {
    assert_eq!(parse_field_type("i8").unwrap(), FieldType::Int("i8"));
    assert_eq!(parse_field_type("u64").unwrap(), FieldType::Int("u64"));
    assert_eq!(parse_field_type("ptr").unwrap(), FieldType::Ptr);
    assert_eq!(parse_field_type("c_char").unwrap(), FieldType::CChar);
    assert_eq!(
        parse_field_type("[u8; 16]").unwrap(),
        FieldType::Array(Box::new(FieldType::Int("u8")), 16)
    );
    assert_eq!(
        parse_field_type("timespec").unwrap(),
        FieldType::Named("timespec".to_string())
    );
}

#[test]
fn rejects_bad_field_types() {
    assert!(parse_field_type("").is_err());
    assert!(parse_field_type("[u8; 0]").is_err());
    assert!(parse_field_type("[u8]").is_err());
    assert!(parse_field_type("unsigned long").is_err());
    assert!(parse_field_type("i128").is_err());
}

#[test]
fn integer_layouts() {
    assert_eq!(int_layout("u8").size, 1);
    assert_eq!(int_layout("i16").align, 2);
    assert_eq!(int_layout("u32").size, 4);
    assert_eq!(int_layout("i64").size, 8);
}

#[test]
fn opaque_element_sizes() {
    assert_eq!(opaque_element(1), ("u8", 1));
    assert_eq!(opaque_element(2), ("u16", 2));
    assert_eq!(opaque_element(4), ("u32", 4));
    assert_eq!(opaque_element(8), ("u64", 8));
    assert_eq!(opaque_element(16), ("u64", 8));
}

#[test]
fn opaque_layout_accepts_a_valid_blob() {
    let file = structs(
        r#"
schema = 1
[[opaque]]
name = "blob"
rust = "Blob"
c = "blob"
size = 128
align = 8
"#,
    );
    let layout = opaque_layout(&file.opaque[0], "x86_64").unwrap();
    assert_eq!(layout.size, 128);
    assert_eq!(layout.align, 8);
}

#[test]
fn opaque_layout_rejects_a_size_that_is_not_a_multiple_of_align() {
    let file = structs(
        r#"
schema = 1
[[opaque]]
name = "blob"
rust = "Blob"
c = "blob"
size = 12
align = 8
"#,
    );
    let error = opaque_layout(&file.opaque[0], "x86_64")
        .unwrap_err()
        .to_string();
    assert!(error.contains("blob"), "{error}");
    assert!(error.contains("multiple"), "{error}");
}

#[test]
fn opaque_layout_rejects_a_bad_alignment() {
    let file = structs(
        r#"
schema = 1
[[opaque]]
name = "blob"
rust = "Blob"
c = "blob"
size = 12
align = 3
"#,
    );
    assert!(opaque_layout(&file.opaque[0], "x86_64").is_err());
}

#[test]
fn opaque_per_arch_layouts() {
    let file = structs(
        r#"
schema = 1
[[opaque]]
name = "ctx"
rust = "Ctx"
c = "ctx"
size = { x86_64 = 936, aarch64 = 4560, riscv64 = 960 }
align = { x86_64 = 8, aarch64 = 16, riscv64 = 16 }
"#,
    );
    assert_eq!(
        opaque_layout(&file.opaque[0], "aarch64").unwrap().size,
        4560
    );
    assert!(!opaque_is_arch_independent(&file.opaque[0]).unwrap());
}

#[test]
fn computes_padded_record() {
    let file = structs(
        r#"
schema = 1
[[record]]
name = "padded"
rust = "Padded"
c = "struct padded"
size = 12
align = 4
fields = [
  { name = "a", ty = "u8", off = 0 },
  { name = "b", ty = "u32", off = 4 },
  { name = "c", ty = "u8", off = 8 },
]
"#,
    );
    let layouts = compute_all(&file.record, &file.opaque, "x86_64").unwrap();
    let record = &layouts.computed["padded"];
    assert_eq!(record.size, 12);
    assert_eq!(record.align, 4);
    assert_eq!(record.fields[1].offset, 4);
}

#[test]
fn computes_nested_record() {
    let file = structs(
        r#"
schema = 1
[[record]]
name = "inner"
rust = "Inner"
c = "struct inner"
size = 8
align = 4
fields = [ { name = "x", ty = "i32", off = 0 }, { name = "y", ty = "i32", off = 4 } ]
[[record]]
name = "outer"
rust = "Outer"
c = "struct outer"
size = 12
align = 4
fields = [ { name = "inner", ty = "inner", off = 0 }, { name = "b", ty = "u8", off = 8 } ]
"#,
    );
    let layouts = compute_all(&file.record, &file.opaque, "x86_64").unwrap();
    assert_eq!(layouts.computed["outer"].size, 12);
}

#[test]
fn computes_array_field() {
    let file = structs(
        r#"
schema = 1
[[record]]
name = "arr"
rust = "Arr"
c = "struct arr"
size = 8
align = 2
fields = [ { name = "v", ty = "[u16; 3]", off = 0 }, { name = "b", ty = "u8", off = 6 } ]
"#,
    );
    let layouts = compute_all(&file.record, &file.opaque, "x86_64").unwrap();
    assert_eq!(layouts.computed["arr"].size, 8);
    assert_eq!(layouts.computed["arr"].fields[0].size, 6);
}

#[test]
fn rejects_a_wrong_offset_naming_the_field() {
    let file = structs(
        r#"
schema = 1
[[record]]
name = "bad"
rust = "Bad"
c = "struct bad"
size = 8
align = 4
fields = [ { name = "a", ty = "u32", off = 0 }, { name = "b", ty = "u32", off = 8 } ]
"#,
    );
    let error = compute_all(&file.record, &file.opaque, "x86_64")
        .unwrap_err()
        .to_string();
    assert!(error.contains("bad"), "{error}");
    assert!(error.contains("field b"), "{error}");
}

#[test]
fn rejects_a_wrong_size_naming_the_record() {
    let file = structs(
        r#"
schema = 1
[[record]]
name = "bad"
rust = "Bad"
c = "struct bad"
size = 9
align = 4
fields = [ { name = "a", ty = "u32", off = 0 } ]
"#,
    );
    let error = compute_all(&file.record, &file.opaque, "x86_64")
        .unwrap_err()
        .to_string();
    assert!(error.contains("bad"), "{error}");
    assert!(error.contains("size"), "{error}");
}

#[test]
fn rejects_a_wrong_alignment() {
    let file = structs(
        r#"
schema = 1
[[record]]
name = "bad"
rust = "Bad"
c = "struct bad"
size = 8
align = 8
fields = [ { name = "a", ty = "u32", off = 0 } ]
"#,
    );
    assert!(compute_all(&file.record, &file.opaque, "x86_64").is_err());
}

#[test]
fn rejects_self_embedding() {
    let file = structs(
        r#"
schema = 1
[[record]]
name = "loop"
rust = "Loop"
c = "struct loop"
size = 8
align = 8
fields = [ { name = "self", ty = "loop", off = 0 } ]
"#,
    );
    let error = compute_all(&file.record, &file.opaque, "x86_64")
        .unwrap_err()
        .to_string();
    assert!(error.contains("embeds itself"), "{error}");
}

#[test]
fn rejects_an_arch_dependent_opaque_inside_a_record() {
    let file = structs(
        r#"
schema = 1
[[opaque]]
name = "ctx"
rust = "Ctx"
c = "ctx"
size = { x86_64 = 8, aarch64 = 16, riscv64 = 16 }
align = { x86_64 = 8, aarch64 = 16, riscv64 = 16 }
[[record]]
name = "outer"
rust = "Outer"
c = "struct outer"
size = 8
align = 8
fields = [ { name = "ctx", ty = "ctx", off = 0 } ]
"#,
    );
    let error = compute_all(&file.record, &file.opaque, "x86_64")
        .unwrap_err()
        .to_string();
    assert!(error.contains("per-architecture"), "{error}");
}

#[test]
fn renders_c_members() {
    let is_record = |name: &str| name == "timespec";
    assert_eq!(
        c_member(&FieldType::Int("u32"), "x", &is_record),
        "unsigned int x"
    );
    assert_eq!(c_member(&FieldType::Ptr, "p", &is_record), "void *p");
    assert_eq!(c_member(&FieldType::CChar, "c", &is_record), "char c");
    assert_eq!(
        c_member(
            &FieldType::Array(Box::new(FieldType::CChar), 8),
            "name",
            &is_record
        ),
        "char name[8]"
    );
    assert_eq!(
        c_member(&FieldType::Named("timespec".to_string()), "t", &is_record),
        "struct __ab_timespec t"
    );
    assert_eq!(
        c_member(&FieldType::Named("sigset_t".to_string()), "s", &is_record),
        "__ab_sigset_t s"
    );
}

#[test]
fn renders_opaque_blob() {
    let file = structs(
        r#"
schema = 1
[[opaque]]
name = "blob"
rust = "Blob"
c = "blob"
size = 16
align = 8
"#,
    );
    let layout = opaque_layout(&file.opaque[0], "x86_64").unwrap();
    assert_eq!(
        c_opaque_blob(&file.opaque[0], layout),
        "typedef struct { _Alignas(8) unsigned char __bytes[16]; } __ab_blob;"
    );
}

#[test]
fn rust_opaque_body_counts_elements() {
    let file = structs(
        r#"
schema = 1
[[opaque]]
name = "blob"
rust = "Blob"
c = "blob"
size = 40
align = 8
"#,
    );
    let body = rust_opaque_body(&file.opaque[0], "x86_64").unwrap();
    assert_eq!(body, ("u64", 5, 40, 8));
}
