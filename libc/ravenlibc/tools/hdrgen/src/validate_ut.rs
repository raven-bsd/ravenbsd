// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Unit tests for table loading and every static rule.

use super::*;
use crate::model::{ConstDef, Deviation, OmitRow, PerArchType};

fn abi_dir() -> std::path::PathBuf {
    std::path::PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../../../.."))
        .join("libc/ravenlibc/abi")
}

fn real() -> Tables {
    load(&abi_dir()).expect("the committed tables are valid")
}

fn constant(name: &str, value: i64) -> ConstDef {
    ConstDef {
        name: name.to_string(),
        value: Some(toml::Value::Integer(value)),
        alias: None,
        or: None,
        shl: None,
        ty: None,
        base: None,
        suffix: None,
        arch: Vec::new(),
        visible: None,
        doc: None,
    }
}

fn deviation(name: &str, against: &str) -> Deviation {
    Deviation {
        name: name.to_string(),
        against: against.to_string(),
        ours: toml::Value::Integer(1),
        theirs: toml::Value::Integer(2),
        reason: "reason".to_string(),
    }
}

#[test]
fn the_committed_tables_validate() {
    let tables = real();
    assert_eq!(tables.groups.len(), 14);
    assert_eq!(tables.structs.record.len(), 26);
    assert_eq!(tables.structs.opaque.len(), 32);
    assert_eq!(tables.types.types.len(), 46);
    assert_eq!(tables.deviations.deviation.len(), 24);
}

#[test]
fn the_constant_count_is_frozen() {
    let tables = real();
    let total: usize = tables
        .groups
        .iter()
        .map(|group| group.def.consts.len())
        .sum();
    assert_eq!(total, 1341);
    let counts: Vec<(&str, usize)> = tables
        .groups
        .iter()
        .map(|group| (group.stem.as_str(), group.def.consts.len()))
        .collect();
    assert_eq!(
        counts,
        vec![
            ("errno", 149),
            ("signal", 116),
            ("fcntl", 111),
            ("mman", 86),
            ("stat", 38),
            ("socket", 235),
            ("netinet", 235),
            ("poll", 12),
            ("time", 25),
            ("resource", 36),
            ("limits", 19),
            ("sysconf", 206),
            ("pthread", 24),
            ("misc", 49),
        ]
    );
}

#[test]
fn values_resolve_for_every_group() {
    let tables = real();
    let values = validate(&tables).expect("values");
    assert_eq!(values.by_name.len(), 1341);
    assert_eq!(values.by_name["EPERM"].value, 1);
    assert_eq!(values.by_name["O_ACCMODE"].value, 0o10000003);
    assert_eq!(values.by_name["RLIM_INFINITY"].value, i128::from(u64::MAX));
}

#[test]
fn duplicate_name_with_overlapping_arches_is_rejected() {
    let mut tables = real();
    tables.groups[1].def.consts.push(constant("EPERM", 1));
    let error = validate(&tables).unwrap_err().to_string();
    assert!(error.contains("overlapping"), "{error}");
}

#[test]
fn duplicate_name_with_disjoint_arches_is_allowed() {
    let mut tables = real();
    let mut left = constant("EXAMPLE_ARCH", 1);
    left.arch = vec!["x86_64".to_string()];
    tables.groups[0].def.consts.push(left);
    let mut right = constant("EXAMPLE_ARCH", 1);
    right.arch = vec!["aarch64".to_string()];
    tables.groups[1].def.consts.push(right);
    validate(&tables).expect("disjoint arch sets may repeat a name");
}

#[test]
fn an_unknown_alias_is_rejected() {
    let mut tables = real();
    let entry = tables.groups[0]
        .def
        .consts
        .iter_mut()
        .find(|constant| constant.name == "EWOULDBLOCK")
        .expect("EWOULDBLOCK");
    entry.alias = Some("NOPE".to_string());
    let error = validate(&tables).unwrap_err().to_string();
    assert!(error.contains("NOPE"), "{error}");
}

#[test]
fn an_alias_cycle_is_rejected() {
    let mut tables = real();
    let entry = tables.groups[0]
        .def
        .consts
        .iter_mut()
        .find(|constant| constant.name == "EAGAIN")
        .expect("EAGAIN");
    entry.value = None;
    entry.alias = Some("EWOULDBLOCK".to_string());
    let error = validate(&tables).unwrap_err().to_string();
    assert!(error.contains("cycle"), "{error}");
}

#[test]
fn a_value_that_does_not_fit_is_rejected() {
    let mut tables = real();
    let entry = tables.groups[0]
        .def
        .consts
        .iter_mut()
        .find(|constant| constant.name == "EPERM")
        .expect("EPERM");
    entry.value = Some(toml::Value::Integer(5_000_000_000));
    let error = validate(&tables).unwrap_err().to_string();
    assert!(error.contains("does not fit"), "{error}");
}

#[test]
fn a_negative_unsigned_value_is_rejected() {
    let mut tables = real();
    tables.groups[0].def.consts.push({
        let mut entry = constant("ENEG", -1);
        entry.ty = Some("u32".to_string());
        entry
    });
    let error = validate(&tables).unwrap_err().to_string();
    assert!(error.contains("unsigned"), "{error}");
}

#[test]
fn an_out_of_range_shift_is_rejected() {
    let mut tables = real();
    let entry = tables.groups[3]
        .def
        .consts
        .iter_mut()
        .find(|constant| constant.name == "MAP_HUGE_16KB")
        .expect("MAP_HUGE_16KB");
    entry.shl = Some([14, 200]);
    let error = validate(&tables).unwrap_err().to_string();
    assert!(error.contains("shift"), "{error}");
}

#[test]
fn an_omit_row_without_a_reason_is_rejected() {
    let mut tables = real();
    tables.groups[0].def.omit.push(OmitRow {
        name: "EXAMPLE".to_string(),
        reason: "  ".to_string(),
    });
    let error = validate(&tables).unwrap_err().to_string();
    assert!(error.contains("empty reason"), "{error}");
}

#[test]
fn a_wrong_header_name_is_rejected() {
    let mut tables = real();
    tables.groups[0].def.c_header = "bits/wrong.h".to_string();
    let error = validate(&tables).unwrap_err().to_string();
    assert!(error.contains("c_header"), "{error}");
}

#[test]
fn a_duplicate_guard_is_rejected() {
    let mut tables = real();
    let guard = tables.groups[0].def.guard.clone();
    tables.groups[1].def.guard = guard;
    let error = validate(&tables).unwrap_err().to_string();
    assert!(error.contains("duplicate guard"), "{error}");
}

#[test]
fn an_absolute_source_path_is_rejected() {
    let mut tables = real();
    tables.groups[0].def.source.push("/etc/passwd".to_string());
    let error = validate(&tables).unwrap_err().to_string();
    assert!(error.contains("not relative"), "{error}");
}

#[test]
fn a_parent_source_path_is_rejected() {
    let mut tables = real();
    tables.groups[0].def.source.push("../outside.h".to_string());
    let error = validate(&tables).unwrap_err().to_string();
    assert!(error.contains("not relative"), "{error}");
}

#[test]
fn per_arch_is_rejected_outside_wchar_and_char() {
    let mut tables = real();
    let ty = tables
        .types
        .types
        .iter_mut()
        .find(|ty| ty.name == "size_t")
        .expect("size_t");
    ty.per_arch.insert(
        "aarch64".to_string(),
        PerArchType {
            rust: "usize".to_string(),
            c: "unsigned long".to_string(),
            signed: false,
        },
    );
    let error = validate(&tables).unwrap_err().to_string();
    assert!(error.contains("per_arch"), "{error}");
}

#[test]
fn deviates_musl_is_rejected_outside_the_frozen_types() {
    let mut tables = real();
    let ty = tables
        .types
        .types
        .iter_mut()
        .find(|ty| ty.name == "size_t")
        .expect("size_t");
    ty.deviates_musl.insert(
        "x86_64".to_string(),
        crate::model::MuslTypeDeviation { bits: 64 },
    );
    let error = validate(&tables).unwrap_err().to_string();
    assert!(error.contains("deviates_musl"), "{error}");
}

#[test]
fn a_wrong_record_offset_is_rejected_by_l1() {
    let mut tables = real();
    let stat = tables
        .structs
        .record
        .iter_mut()
        .find(|record| record.name == "stat")
        .expect("stat");
    stat.fields[0].off = 1;
    let error = validate(&tables).unwrap_err().to_string();
    assert!(error.contains("field st_dev"), "{error}");
}

#[test]
fn an_opaque_size_that_is_not_a_multiple_of_align_is_rejected() {
    let mut tables = real();
    let jmp = tables
        .structs
        .opaque
        .iter_mut()
        .find(|opaque| opaque.name == "jmp_buf")
        .expect("jmp_buf");
    jmp.size = crate::model::IntOrTable::Int(393);
    let error = validate(&tables).unwrap_err().to_string();
    assert!(error.contains("jmp_buf"), "{error}");
}

#[test]
fn an_unknown_deviation_name_is_rejected() {
    let mut tables = real();
    tables.deviations.deviation.push(deviation("NOPE", "musl"));
    let error = validate(&tables).unwrap_err().to_string();
    assert!(error.contains("NOPE"), "{error}");
}

#[test]
fn an_empty_deviation_reason_is_rejected() {
    let mut tables = real();
    let mut row = deviation("SOMAXCONN", "musl");
    row.reason = " ".to_string();
    tables.deviations.deviation.push(row);
    let error = validate(&tables).unwrap_err().to_string();
    assert!(error.contains("empty reason"), "{error}");
}

#[test]
fn a_bad_schema_is_rejected() {
    let mut tables = real();
    tables.types.schema = 2;
    let error = validate(&tables).unwrap_err().to_string();
    assert!(error.contains("schema"), "{error}");
}

#[test]
fn a_missing_required_type_is_rejected() {
    let mut tables = real();
    tables.types.types.retain(|ty| ty.name != "size_t");
    let error = validate(&tables).unwrap_err().to_string();
    assert!(error.contains("size_t"), "{error}");
}

#[test]
fn a_type_bits_mismatch_is_rejected() {
    let mut tables = real();
    let ty = tables
        .types
        .types
        .iter_mut()
        .find(|ty| ty.name == "size_t")
        .expect("size_t");
    ty.bits = 32;
    let error = validate(&tables).unwrap_err().to_string();
    assert!(error.contains("bits"), "{error}");
}

#[test]
fn a_duplicate_record_name_is_rejected() {
    let mut tables = real();
    let duplicate = tables.structs.record[0].clone();
    tables.structs.record.push(duplicate);
    let error = validate(&tables).unwrap_err().to_string();
    assert!(error.contains("duplicate type"), "{error}");
}

#[test]
fn two_value_forms_are_rejected() {
    let mut tables = real();
    let entry = tables.groups[0]
        .def
        .consts
        .iter_mut()
        .find(|constant| constant.name == "EPERM")
        .expect("EPERM");
    entry.alias = Some("ENOENT".to_string());
    let error = validate(&tables).unwrap_err().to_string();
    assert!(error.contains("exactly one"), "{error}");
}

#[test]
fn an_invalid_visibility_is_rejected() {
    let mut tables = real();
    let entry = tables.groups[0]
        .def
        .consts
        .iter_mut()
        .find(|constant| constant.name == "EPERM")
        .expect("EPERM");
    entry.visible = Some("beos".to_string());
    let error = validate(&tables).unwrap_err().to_string();
    assert!(error.contains("visibility"), "{error}");
}

#[test]
fn an_invalid_suffix_is_rejected() {
    let mut tables = real();
    let entry = tables.groups[0]
        .def
        .consts
        .iter_mut()
        .find(|constant| constant.name == "EPERM")
        .expect("EPERM");
    entry.suffix = Some("Z".to_string());
    let error = validate(&tables).unwrap_err().to_string();
    assert!(error.contains("suffix"), "{error}");
}

#[test]
fn an_invalid_base_is_rejected() {
    let mut tables = real();
    let entry = tables.groups[0]
        .def
        .consts
        .iter_mut()
        .find(|constant| constant.name == "EPERM")
        .expect("EPERM");
    entry.base = Some("binary".to_string());
    let error = validate(&tables).unwrap_err().to_string();
    assert!(error.contains("base"), "{error}");
}

#[test]
fn a_constant_and_an_omit_row_cannot_share_a_name() {
    let mut tables = real();
    tables.groups[0].def.omit.push(OmitRow {
        name: "EPERM".to_string(),
        reason: "no".to_string(),
    });
    let error = validate(&tables).unwrap_err().to_string();
    assert!(error.contains("both"), "{error}");
}

#[test]
fn a_constant_row_without_integer_values_is_rejected() {
    let mut tables = real();
    let row = tables
        .deviations
        .deviation
        .iter_mut()
        .find(|row| row.name == "O_LARGEFILE" && row.against == "musl")
        .expect("O_LARGEFILE");
    row.ours = toml::Value::String("128/8".to_string());
    let error = validate(&tables).unwrap_err().to_string();
    assert!(error.contains("integer"), "{error}");
}

#[test]
fn group_name_must_equal_the_file_stem() {
    let mut tables = real();
    tables.groups[0].def.group = "other".to_string();
    let error = validate(&tables).unwrap_err().to_string();
    assert!(error.contains("file stem"), "{error}");
}

#[test]
fn header_names_follow_the_table() {
    assert_eq!(expected_header("errno"), "bits/errno.h");
    assert_eq!(expected_header("signal"), "bits/signum.h");
    assert_eq!(expected_header("sysconf"), "bits/sysconf.h");
    assert_eq!(expected_header("fcntl"), "bits/abi_fcntl.h");
}

#[test]
fn type_kinds_classifies_every_name() {
    let tables = real();
    let kinds = type_kinds(&tables);
    assert_eq!(kinds["stat"], Kind::Layout);
    assert_eq!(kinds["EPERM"], Kind::Constant);
    assert_eq!(kinds["jmp_buf"], Kind::Layout);
}

#[test]
fn helper_lookups_find_the_rows() {
    let tables = real();
    assert_eq!(type_def(&tables, "nlink_t").expect("nlink_t").bits, 32);
    assert!(matches!(
        field_type(&tables, "stat", "st_size").expect("field"),
        crate::layout::FieldType::Int("i64")
    ));
    assert_eq!(rust_name(&tables, "stat"), Some("Stat"));
    assert_eq!(rust_name(&tables, "sigset_t"), Some("SigsetT"));
    assert!(rust_name(&tables, "nope").is_none());
}
