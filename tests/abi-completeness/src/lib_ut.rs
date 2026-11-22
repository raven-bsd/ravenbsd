// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Tests for the ABI completeness model.

use super::*;

fn entry(number: u64, name: &str) -> Entry {
    Entry {
        number,
        name: name.to_string(),
        site: String::new(),
    }
}

fn focus_model() -> Model {
    let mut dispatch = vec![entry(0x0D, "PROC_CHROOT"), entry(0x46, "PROC_REBOOT")];
    let mut documented = Vec::new();
    let mut known = BTreeSet::new();
    for (number, name) in [(0x0D, "PROC_CHROOT"), (0x46, "PROC_REBOOT")] {
        known.insert(entry(number, name));
    }
    dispatch.push(entry(0x01, "EXIT"));
    documented.push(entry(0x01, "EXIT"));
    Model {
        dispatch,
        documented,
        known,
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
    model.known.insert(entry(0x02, "YIELD"));
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
fn dispatch_and_documented_table_are_complete() {
    let root = root();
    let model = model(&root);
    assert!(
        model.dispatch.len() >= 100,
        "dispatch surface parsed only {} entries",
        model.dispatch.len()
    );
    let errors = check(&model);
    assert!(
        errors.is_empty(),
        "ABI completeness failures ({}):\n{}",
        errors.len(),
        errors.join("\n")
    );
}
