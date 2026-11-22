// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Tests for the ABI completeness model.

use std::collections::BTreeMap;
use std::path::PathBuf;

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
    let doc = std::fs::read_to_string(mgmt_file(&root, "architecture/product-data/syscall-abi.md"))
        .expect("doc");
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
fn wave_syscalls_are_in_the_served_table() {
    let root = root();
    let doc = std::fs::read_to_string(mgmt_file(&root, "architecture/product-data/syscall-abi.md"))
        .expect("doc");
    let model = model(&root);
    for (number, name, reason) in [
        (0x08_u64, "GETRANDOM", "public: libc getrandom/getentropy"),
        (0x110, "DUPLEX_PAIR", "public: libc socketpair"),
        (0x111, "DUPLEX_SHUTDOWN", "public: libc shutdown"),
        (0x112, "DUPLEX_SENDMSG", "public: libc sendmsg"),
        (0x113, "DUPLEX_RECVMSG", "public: libc recvmsg"),
        (
            0x114,
            "DUPLEX_PEERCRED",
            "public: libc getpeereid/getpeercred",
        ),
        (0x1E1, "PROC_CTTY_GET", "public: libc tcgetpgrp"),
        (0x1E2, "PROC_CTTY_SET_FG", "public: libc tcsetpgrp"),
    ] {
        let hex = format!("0x{number:02X}");
        let key = (number, name.to_string());
        assert_eq!(
            model.known.get(&key).map(String::as_str),
            Some(reason),
            "served known-omission reason for {hex} {name}"
        );
        let comment = format!("<!-- abi-completeness: known-omission {hex} {name} {reason} -->");
        assert!(doc.contains(&comment), "served table missing: {comment}");
        assert!(
            doc.contains(&format!("| `{hex}` | `{name}` |")),
            "served table row missing: {hex} {name}"
        );
        assert!(
            model
                .dispatch
                .iter()
                .any(|entry| entry.number == number && entry.name == name),
            "dispatch surface missing: {hex} {name}"
        );
    }
    assert!(doc.contains("`AT_RANDOM`"), "AT_RANDOM note missing");
    assert!(
        doc.contains("not a syscall number"),
        "AT_RANDOM must be documented as an auxv entry, not a syscall"
    );
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

fn mgmt_temp(label: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!("raven-abi-mgmt-{}-{label}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("temp root");
    root
}

#[test]
fn mgmt_root_honours_the_env_override() {
    let root = mgmt_temp("env");
    let product = root.join("product");
    let mgmt = root.join("mgmt");
    std::fs::create_dir_all(&product).expect("product");
    std::fs::create_dir_all(&mgmt).expect("mgmt");
    let resolved =
        resolve_mgmt_root(&product, Some(mgmt.to_str().expect("utf8"))).expect("resolve");
    assert_eq!(resolved, mgmt);
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn mgmt_root_defaults_to_the_sibling() {
    let root = mgmt_temp("sibling");
    let product = root.join("ravenbsd");
    let mgmt = root.join("ravenbsd-prjmgmt");
    std::fs::create_dir_all(&product).expect("product");
    std::fs::create_dir_all(&mgmt).expect("mgmt");
    let resolved = resolve_mgmt_root(&product, Some("")).expect("resolve");
    assert_eq!(resolved, mgmt);
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn mgmt_root_names_env_when_the_sibling_is_missing() {
    let root = mgmt_temp("no-sibling");
    let product = root.join("ravenbsd");
    std::fs::create_dir_all(&product).expect("product");
    let error = resolve_mgmt_root(&product, Some("")).expect_err("missing sibling");
    assert!(
        error.contains("RAVEN_MGMT_ROOT"),
        "error must name RAVEN_MGMT_ROOT: {error}"
    );
    assert!(
        error.contains("ravenbsd-prjmgmt"),
        "error must name the expected tree: {error}"
    );
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn mgmt_root_rejects_a_missing_env_path() {
    let root = mgmt_temp("missing-env");
    let product = root.join("ravenbsd");
    std::fs::create_dir_all(&product).expect("product");
    let missing = root.join("missing");
    let error = resolve_mgmt_root(&product, Some(missing.to_str().expect("utf8")))
        .expect_err("missing env root");
    assert!(
        error.contains("RAVEN_MGMT_ROOT"),
        "error must name RAVEN_MGMT_ROOT: {error}"
    );
    let _ = std::fs::remove_dir_all(&root);
}
