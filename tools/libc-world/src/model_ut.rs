// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

use super::*;

#[test]
fn elf_machines_match_the_trap_list() {
    assert_eq!(elf_machine("x86_64"), Some(0x3e));
    assert_eq!(elf_machine("aarch64"), Some(0xb7));
    assert_eq!(elf_machine("riscv64"), Some(0xf3));
    assert_eq!(elf_machine("mips"), None);
}

#[test]
fn triples_use_the_crate_layout_spelling() {
    assert_eq!(triple("x86_64").as_deref(), Some("x86_64-unknown-ravenbsd"));
    assert_eq!(
        linux_triple("aarch64").as_deref(),
        Some("aarch64-unknown-linux-gnu")
    );
    assert!(triple("sparc").is_none());
}

#[test]
fn every_arch_has_a_triple_and_a_machine() {
    for arch in ARCHES {
        assert!(triple(arch).is_some(), "{arch}");
        assert!(elf_machine(arch).is_some(), "{arch}");
    }
}

#[test]
fn a_new_result_is_not_built() {
    let result = PackageResult::new(25, "zlib", "1.3.1", "built");
    assert!(result.is_built());
    assert_eq!(result.tests, "not-run");
    assert_eq!(result.libc_patches, 0);
    let placeholder = PackageResult::new(99, "sqlite", "PLACEHOLDER", "placeholder");
    assert!(!placeholder.is_built());
}

#[test]
fn result_json_has_the_fixed_field_order() {
    let result = PackageResult::new(25, "zlib", "1.3.1", "built");
    let text = serde_json::to_string(&result).expect("json");
    let name = text.find("\"name\"").expect("name");
    let n = text.find("\"n\"").expect("n");
    let version = text.find("\"version\"").expect("version");
    let status = text.find("\"status\"").expect("status");
    assert!(name < n && n < version && version < status);
    assert!(text.ends_with("\"steps\":[]}"));
}

#[test]
fn result_json_round_trips() {
    let mut result = PackageResult::new(25, "zlib", "1.3.1", "built");
    result.findings.insert("missing-symbol".to_string(), 2);
    result.steps.push(Step {
        name: "make".to_string(),
        exit: Some(0),
    });
    let text = serde_json::to_string(&result).expect("json");
    let back: PackageResult = serde_json::from_str(&text).expect("parse");
    assert_eq!(back, result);
}

#[test]
fn failure_round_trips() {
    let failure = Failure {
        class: "missing-header".to_string(),
        detail: "sys/epoll.h".to_string(),
    };
    let text = serde_json::to_string(&failure).expect("json");
    let back: Failure = serde_json::from_str(&text).expect("parse");
    assert_eq!(back, failure);
}

#[test]
fn status_tokens_are_unique() {
    let unique: std::collections::BTreeSet<_> = STATUSES.iter().collect();
    assert_eq!(unique.len(), STATUSES.len());
}

#[test]
fn implemented_builders_are_a_subset_of_all_keys() {
    for key in IMPLEMENTED_BUILDERS {
        assert!(BUILDERS.contains(key), "{key}");
    }
    assert!(!IMPLEMENTED_BUILDERS.contains(&"me"));
}

#[test]
fn failure_classes_are_unique() {
    let unique: std::collections::BTreeSet<_> = FAILURE_CLASSES.iter().collect();
    assert_eq!(unique.len(), FAILURE_CLASSES.len());
}
