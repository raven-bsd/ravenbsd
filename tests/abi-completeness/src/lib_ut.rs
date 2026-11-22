// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Tests for the ABI completeness model.

use std::collections::BTreeMap;

use super::*;

fn entry(number: u64, name: &str) -> Entry {
    Entry {
        number,
        name: name.to_string(),
        site: String::new(),
    }
}

fn focus_model() -> Model {
    let dispatch = vec![
        entry(0x0D, "PROC_CHROOT"),
        entry(0x46, "PROC_REBOOT"),
        entry(0x01, "EXIT"),
    ];
    let documented = vec![
        entry(0x0D, "PROC_CHROOT"),
        entry(0x46, "PROC_REBOOT"),
        entry(0x01, "EXIT"),
    ];
    Model {
        dispatch,
        documented,
        known: BTreeMap::new(),
        reserved: vec![(0, 0x1000)],
    }
}

#[test]
fn a_missing_documented_row_is_named_with_its_number() {
    let mut model = focus_model();
    model.dispatch.push(entry(0x02, "YIELD"));
    let errors = check(&model);
    assert!(
        errors
            .iter()
            .any(|error| error.contains("missing from the documented table: 0x02 YIELD")),
        "{errors:?}"
    );
}

#[test]
fn a_duplicate_documented_number_fails() {
    let mut model = focus_model();
    model.documented.push(entry(0x01, "EXIT_AGAIN"));
    let errors = check(&model);
    assert!(
        errors
            .iter()
            .any(|error| error.contains("duplicate documented number 0x01")),
        "{errors:?}"
    );
}

#[test]
fn a_stale_known_omission_fails() {
    let mut model = focus_model();
    model.dispatch.push(entry(0x02, "YIELD"));
    model
        .known
        .insert((0x02, "YIELD".to_string()), "private: test".to_string());
    model.documented.push(entry(0x02, "YIELD"));
    let errors = check(&model);
    assert!(
        errors
            .iter()
            .any(|error| error.contains("stale known omission: 0x02 YIELD")),
        "{errors:?}"
    );
}

#[test]
fn a_known_omission_without_a_reason_fails() {
    let mut model = focus_model();
    model.dispatch.push(entry(0x02, "YIELD"));
    model
        .known
        .insert((0x02, "YIELD".to_string()), String::new());
    let errors = check(&model);
    assert!(
        errors.iter().any(|error| error
            .contains("known omission 0x02 YIELD has no machine-readable reason")),
        "{errors:?}"
    );
}

#[test]
fn an_undocumented_gap_fails() {
    let mut model = focus_model();
    model.reserved.clear();
    let errors = check(&model);
    assert!(
        errors
            .iter()
            .any(|error| error.contains("undocumented gap at 0x00")),
        "{errors:?}"
    );
}

#[test]
fn focus_rows_are_regenerated_with_real_numbers() {
    let root = root();
    let doc = std::fs::read_to_string(root.join("docs/syscall-abi.md")).expect("doc");
    let snapshot = std::fs::read_to_string(root.join("tests/abi/abi-v1.txt")).expect("snapshot");
    for (number, name, argc, ret) in [
        ("0x0D", "PROC_CHROOT", "2", "0"),
        ("0x46", "PROC_REBOOT", "1", "!"),
    ] {
        let row = format!("| `{number}` | `{name}` | {argc} | `{ret}` |");
        assert!(doc.contains(&row), "documented row missing: {row}");
        let line = format!(
            "SYSCALL {} {} argc={} ret={}",
            number.to_ascii_lowercase(),
            name,
            argc,
            ret
        );
        assert!(snapshot.contains(&line), "snapshot line missing: {line}");
    }
}

#[test]
fn dispatch_and_documented_table_are_complete() {
    let root = root();
    let model = model(&root);
    let errors = check(&model);
    assert!(
        errors.is_empty(),
        "ABI completeness failures ({}):\n{}",
        errors.len(),
        errors.join("\n")
    );
    assert!(
        model.dispatch.len() >= 100,
        "dispatch surface parsed only {} entries",
        model.dispatch.len()
    );
    assert!(
        model.documented.len() >= 63,
        "documented table parsed only {} rows",
        model.documented.len()
    );
    assert_eq!(
        model.known.len(),
        43,
        "the kernel-only omission register must hold 43 rows"
    );
}
