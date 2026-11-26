// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Unit tests for the FreeBSD oracle's parsing and selection rules.

use super::*;

#[test]
fn family_selection_accepts_all_and_lists() {
    let all = Selection::parse("all").expect("all");
    assert!(all.all);
    assert!(all.wants("errno"));
    assert!(all.wants("records"));
    let some = Selection::parse("errno, signal").expect("list");
    assert!(!some.all);
    assert!(some.wants("errno"));
    assert!(some.wants("signal"));
    assert!(!some.wants("fcntl"));
}

#[test]
fn family_selection_rejects_empty_and_misspelled_names() {
    let empty = Selection::parse("").expect_err("empty must fail");
    assert!(empty.to_string().contains("at least one family"), "{empty}");
    let blank = Selection::parse("errno,").expect_err("blank entry must fail");
    assert!(blank.to_string().contains("empty entry"), "{blank}");
    let unknown = Selection::parse("errnoo").expect_err("typo must fail");
    assert!(unknown.to_string().contains("unknown family"), "{unknown}");
}

#[test]
fn targets_cover_both_freebsd_arches_and_reject_riscv64() {
    assert_eq!(targets("all").expect("all"), vec!["x86_64", "aarch64"]);
    assert_eq!(targets("aarch64").expect("aarch64"), vec!["aarch64"]);
    let error = targets("riscv64").expect_err("riscv64 is not a FreeBSD target");
    assert!(
        error.to_string().contains("unknown FreeBSD --target"),
        "{error}"
    );
}

#[test]
fn record_layout_parser_keeps_named_top_level_fields_only() {
    let dump = "\
*** Dumping AST Record Layout
         0 | struct stat
         0 |   dev_t st_dev
         8 |   ino_t st_ino
        16 |   struct timespec st_atim
        16 |     time_t tv_sec
        24 |     long tv_nsec
        32 |   __uint64_t[9] st_spare
       104:- |   int
           | [sizeof=224, align=8]
";
    let wanted = BTreeSet::from(["struct stat".to_string()]);
    let layouts = parse_record_layouts(dump, &wanted);
    let stat = layouts.get("struct stat").expect("struct stat");
    assert_eq!(stat.size, 224);
    assert_eq!(stat.align, 8);
    let names: Vec<&str> = stat
        .fields
        .iter()
        .map(|field| field.name.as_str())
        .collect();
    assert_eq!(names, vec!["st_dev", "st_ino", "st_atim", "st_spare"]);
    assert_eq!(stat.fields[2].offset, 16);
    assert_eq!(stat.fields[3].ty, "__uint64_t[9]");
}

#[test]
fn record_layout_parser_ignores_unrequested_records() {
    let dump = "\
*** Dumping AST Record Layout
         0 | struct timespec
         0 |   time_t tv_sec
           | [sizeof=16, align=8]

*** Dumping AST Record Layout
         0 | struct pollfd
         0 |   int fd
           | [sizeof=8, align=4]
";
    let wanted = BTreeSet::from(["struct pollfd".to_string()]);
    let layouts = parse_record_layouts(dump, &wanted);
    assert!(!layouts.contains_key("struct timespec"));
    assert_eq!(layouts.get("struct pollfd").expect("pollfd").size, 8);
}

#[test]
fn rust_constant_parser_reads_types_hex_and_skips_non_integers() {
    let text = "\
pub const EAGAIN: i32 = 11;
pub const O_CREAT: i32 = 0x200;
pub const CHAR_IS_SIGNED: bool = true;
pub const NEGATIVE: i64 = -7;
";
    let values = parse_rust_constants(text);
    assert_eq!(values.get("EAGAIN"), Some(&11));
    assert_eq!(values.get("O_CREAT"), Some(&512));
    assert_eq!(values.get("NEGATIVE"), Some(&-7));
    assert!(!values.contains_key("CHAR_IS_SIGNED"));
}

#[test]
fn freebsd_version_is_read_from_param_h() {
    assert_eq!(
        freebsd_version("#define __FreeBSD_version 1400526\n#define __FreeBSD 14\n"),
        Some(1_400_526)
    );
    assert_eq!(freebsd_version("#define OTHER 1\n"), None);
}

#[test]
fn rust_layout_parser_reads_size_align_and_offsets() {
    let text = "\
const _: () =
    assert!(core::mem::size_of::<Timespec>() == 16 && core::mem::align_of::<Timespec>() == 8);
const _: () = assert!(core::mem::offset_of!(Timespec, tv_sec) == 0);
const _: () = assert!(core::mem::offset_of!(Timespec, tv_nsec) == 8);
const _: () = assert!(core::mem::size_of::<MqAttr>() == 64 && core::mem::align_of::<MqAttr>() == 8);
";
    let layouts = parse_rust_layouts(text);
    let timespec = layouts.get("Timespec").expect("Timespec");
    assert_eq!(timespec.size, Some(16));
    assert_eq!(timespec.align, Some(8));
    assert_eq!(timespec.fields.get("tv_nsec"), Some(&8));
    assert_eq!(layouts.get("MqAttr").expect("MqAttr").size, Some(64));
}

#[test]
fn runtime_accessor_is_classified_apart_from_literals() {
    assert!(RUNTIME_ACCESSORS.contains(&"CLK_TCK"));
}

fn constant(name: &str, value: i128) -> ConstDef {
    ConstDef {
        name: name.to_string(),
        value: Some(toml::Value::Integer(i64::try_from(value).unwrap_or(0))),
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

fn alias(name: &str, target: &str) -> ConstDef {
    ConstDef {
        name: name.to_string(),
        value: None,
        alias: Some(target.to_string()),
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

fn test_tables() -> Tables {
    use crate::model::{ConstFile, DeviationFile, StructsFile, TypesFile};
    let group = |stem: &str, consts: Vec<ConstDef>| crate::validate::Group {
        stem: stem.to_string(),
        def: ConstFile {
            schema: 1,
            group: stem.to_string(),
            c_header: format!("bits/{stem}.h"),
            guard: format!("_{stem}_H"),
            rust_type: "i32".to_string(),
            base: "dec".to_string(),
            source: vec!["include/errno.h".to_string()],
            donor: None,
            donor_absent: None,
            prefixes: Vec::new(),
            omit: Vec::new(),
            consts,
        },
    };
    Tables {
        types: TypesFile {
            schema: 1,
            types: vec![crate::model::TypeDef {
                name: "sa_family_t".to_string(),
                rust: "u8".to_string(),
                c: "unsigned char".to_string(),
                bits: 8,
                signed: false,
                per_arch: BTreeMap::new(),
                deviates_musl: BTreeMap::new(),
                ctype_probe: None,
                doc: None,
            }],
        },
        structs: StructsFile {
            schema: 1,
            record: vec![RecordDef {
                name: "stat".to_string(),
                rust: "Stat".to_string(),
                c: "struct stat".to_string(),
                musl: None,
                size: 224,
                align: 8,
                deviates: BTreeMap::new(),
                fields: vec![crate::model::FieldDef {
                    name: "st_size".to_string(),
                    ty: "i64".to_string(),
                    off: 112,
                    musl_field: None,
                    doc: None,
                }],
                doc: None,
            }],
            opaque: Vec::new(),
        },
        groups: vec![
            group("errno", vec![constant("EAGAIN", 11)]),
            group(
                "fcntl",
                vec![
                    constant("O_SYNC", 128),
                    alias("O_RSYNC", "O_SYNC"),
                    constant("O_EXEC", 0x20_0000),
                    constant("O_PATH", 0x20_0000),
                ],
            ),
        ],
        deviations: DeviationFile {
            schema: 1,
            deviation: Vec::new(),
        },
    }
}

fn side_constants(pairs: &[(&str, i128)]) -> Side {
    let mut side = Side::default();
    for (name, value) in pairs {
        side.constants.insert((*name).to_string(), *value);
    }
    side
}

#[test]
fn comparison_reports_every_required_negative_class() {
    let tables = test_tables();
    let mut donor = side_constants(&[
        ("EAGAIN", 35),
        ("O_SYNC", 128),
        ("O_RSYNC", 128),
        ("O_EXEC", 0x20_0000),
        ("O_PATH", 0x20_0000),
    ]);
    donor.types.insert(
        "sa_family_t".to_string(),
        TypeFacts {
            size: 1,
            align: 1,
            signed: false,
        },
    );
    donor.records.insert(
        "stat".to_string(),
        RecordFacts {
            size: 224,
            align: 8,
            fields: vec![FieldFacts {
                name: "st_size".to_string(),
                offset: 112,
                ty: "off_t".to_string(),
                size: Some(8),
                signed: Some(true),
                bitfield: false,
            }],
        },
    );
    let mut raven = side_constants(&[
        ("EAGAIN", 11),
        ("O_SYNC", 128),
        ("O_RSYNC", 1052672),
        ("O_EXEC", 0x20_0000),
    ]);
    raven.types.insert(
        "sa_family_t".to_string(),
        TypeFacts {
            size: 2,
            align: 2,
            signed: false,
        },
    );
    raven.records.insert(
        "stat".to_string(),
        RecordFacts {
            size: 128,
            align: 4,
            fields: vec![FieldFacts {
                name: "st_size".to_string(),
                offset: 48,
                ty: "long".to_string(),
                size: Some(4),
                signed: Some(false),
                bitfield: false,
            }],
        },
    );
    let mut rust = BTreeMap::new();
    rust.insert("EAGAIN".to_string(), 12);
    rust.insert("O_SYNC".to_string(), 129);
    let mut inventory = Inventory::default();
    compare(
        &tables,
        "x86_64",
        &donor,
        &raven,
        &rust,
        &BTreeMap::new(),
        &mut inventory,
    );
    let kinds: BTreeSet<&str> = inventory.diffs.iter().map(|diff| diff.kind).collect();
    for kind in [
        "constant",
        "constant-missing",
        "rust-constant",
        "type-size",
        "record-size",
        "record-align",
        "field-offset",
        "field-size",
        "field-signed",
        "alias-relation",
    ] {
        assert!(kinds.contains(kind), "missing {kind}: {kinds:?}");
    }
}

#[test]
fn comparison_has_no_difference_when_every_measurement_matches() {
    let tables = test_tables();
    let mut donor = side_constants(&[
        ("EAGAIN", 11),
        ("O_SYNC", 128),
        ("O_RSYNC", 128),
        ("O_EXEC", 0x20_0000),
        ("O_PATH", 0x20_0000),
    ]);
    donor.types.insert(
        "sa_family_t".to_string(),
        TypeFacts {
            size: 1,
            align: 1,
            signed: false,
        },
    );
    donor.records.insert(
        "stat".to_string(),
        RecordFacts {
            size: 224,
            align: 8,
            fields: Vec::new(),
        },
    );
    let mut raven = side_constants(&[
        ("EAGAIN", 11),
        ("O_SYNC", 128),
        ("O_RSYNC", 128),
        ("O_EXEC", 0x20_0000),
        ("O_PATH", 0x20_0000),
    ]);
    raven.types.insert(
        "sa_family_t".to_string(),
        TypeFacts {
            size: 1,
            align: 1,
            signed: false,
        },
    );
    raven.records.insert(
        "stat".to_string(),
        RecordFacts {
            size: 224,
            align: 8,
            fields: Vec::new(),
        },
    );
    let mut rust = BTreeMap::new();
    for (name, value) in &donor.constants {
        rust.insert(name.clone(), *value);
    }
    donor.runtime.insert("CLK_TCK".to_string(), Ok(()));
    raven.runtime.insert("CLK_TCK".to_string(), Ok(()));
    let mut inventory = Inventory::default();
    compare(
        &tables,
        "x86_64",
        &donor,
        &raven,
        &rust,
        &BTreeMap::new(),
        &mut inventory,
    );
    assert!(inventory.diffs.is_empty(), "{:?}", inventory.diffs);
}

#[test]
fn the_rust_oracle_reads_the_evaluated_all_table() {
    let text = "\
pub const POLLWRNORM: i32 = POLLOUT;
pub const ALL: &[(&str, i128)] = &[
    (\"POLLOUT\", 4i128),
    (\"POLLWRNORM\", 4i128),
    (\"POLLSTANDARD\", 511i128),
];
";
    let values = parse_rust_constants(text);
    assert_eq!(values.get("POLLOUT"), Some(&4));
    assert_eq!(values.get("POLLWRNORM"), Some(&4));
    assert_eq!(values.get("POLLSTANDARD"), Some(&511));
}

#[test]
fn a_declared_donor_absent_policy_approves_an_extension() {
    let mut tables = test_tables();
    let diff = Diff {
        family: "fcntl".to_string(),
        kind: "donor-absent",
        name: "O_TMPFILE".to_string(),
        arch: "x86_64".to_string(),
        expected: Value::Text("absent".to_string()),
        actual: Value::Int(0x410000),
        detail: String::new(),
        approved: false,
    };
    assert!(!approved(&tables, &diff));
    tables.groups[1].def.donor_absent = Some("Raven extension".to_string());
    assert!(approved(&tables, &diff));
    let wrong = Diff {
        expected: Value::Text("something else".to_string()),
        ..diff
    };
    assert!(!approved(&tables, &wrong));
}
