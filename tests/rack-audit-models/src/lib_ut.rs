// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Audit tests: model/donor rows, constants, traces, K315 and known product gaps.

use std::fs;
use std::path::PathBuf;

use crate::audit::{load, trace_rows, validate, validate_constants};

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn audit_rows() -> Vec<crate::audit::Row> {
    load(&root().join("tests/rack-audit-models/audit.tsv")).expect("audit manifest")
}

#[test]
fn all_five_model_crates_are_present() {
    for crate_name in [
        "rack-loss-model",
        "rack-tlp-model",
        "rack-recovery-model",
        "rack-bbr-model",
        "rack-vtable-model",
    ] {
        assert!(
            root().join("tests").join(crate_name).is_dir(),
            "missing {crate_name}"
        );
    }
}

#[test]
fn audit_rows_match_sources_tests_mutations_and_k315() {
    let report = validate(&root(), &audit_rows());
    assert_eq!(report.rows, 19);
    assert!(report.gaps.is_empty(), "{:#?}", report.gaps);
}

#[test]
fn donor_constants_match_the_model_sources() {
    let gaps = validate_constants(&root());
    assert!(gaps.is_empty(), "{gaps:#?}");
}

#[test]
fn k319_and_k324_traces_have_ten_thousand_rows_each() {
    assert_eq!(
        trace_rows(&root().join("tests/rack-loss-model/traces/rack-10k.tsv")),
        10_000
    );
    assert_eq!(
        trace_rows(&root().join("tests/rack-recovery-model/traces/rack-recovery-10k.tsv")),
        10_000
    );
}

#[test]
fn k320_k326_k327_have_randomized_model_tests() {
    let root = root();
    let tlp =
        fs::read_to_string(root.join("tests/rack-tlp-model/src/lib_ut.rs")).expect("K320 tests");
    assert!(tlp.contains("fn randomized_timers_and_sack_traces_are_deterministic_and_invariant"));
    let bbr =
        fs::read_to_string(root.join("tests/rack-bbr-model/src/lib_ut.rs")).expect("K326 tests");
    assert!(bbr.contains("fn ten_thousand_randomized_trace_steps_are_deterministic_and_bounded"));
    let vtable =
        fs::read_to_string(root.join("tests/rack-vtable-model/src/lib_ut.rs")).expect("K327 tests");
    assert!(vtable.contains("fn ten_thousand_randomized_events_are_deterministic_and_invariant"));
}

#[test]
fn k315_manifest_has_219_unique_functions() {
    let text = fs::read_to_string(root().join("tests/rack-coverage/fixtures/rack-functions.txt"))
        .expect("K315 manifest");
    let mut names = std::collections::BTreeSet::new();
    for line in text.lines() {
        if line.starts_with('#') || line.trim().is_empty() {
            continue;
        }
        if let Some(name) = line.split_whitespace().nth(1) {
            names.insert(name);
        }
    }
    assert_eq!(names.len(), 219);
}

#[test]
fn known_product_model_gaps_are_recorded() {
    let replay = fs::read_to_string(root().join("docs/rack-replay.md")).expect("K340 replay");
    for expected in [
        "expected lost `4000-5000` pipe `2000`",
        "expected lost `-` pipe `1000`",
        "expected `reo_wnd 25`, got `reo_wnd 50`",
        "expected `6` (`TCP_FUNC_OUTPUT_CANDROP|TCP_FUNC_DEFAULT_OK`), got `3`",
    ] {
        assert!(replay.contains(expected), "missing replay gap {expected:?}");
    }

    let model = fs::read_to_string(root().join("tests/rack-vtable-model/src/model.rs"))
        .expect("K327 model");
    let product = fs::read_to_string(root().join("crates/raven-tcpip/src/rack_vtable.rs"))
        .expect("rack_vtable product");
    assert!(model.contains("pub const TCP_FUNC_OUTPUT_CANDROP: u32 = 0x02;"));
    assert!(model.contains("pub const TCP_FUNC_DEFAULT_OK: u32 = 0x04;"));
    assert!(product.contains("pub const TCP_FUNC_OUTPUT_CANDROP: u32 = 0x0001;"));
    assert!(product.contains("pub const TCP_FUNC_DEFAULT_OK: u32 = 0x0002;"));
}

#[test]
fn a_corrupted_audit_row_is_detected() {
    let mut rows = audit_rows();
    rows[0].token = String::from("token-that-does-not-exist");
    let report = validate(&root(), &rows);
    assert!(!report.gaps.is_empty());
}

#[test]
fn a_duplicate_audit_row_is_detected() {
    let mut rows = audit_rows();
    rows.push(rows[0].clone());
    let report = validate(&root(), &rows);
    assert!(
        report
            .gaps
            .iter()
            .any(|gap| gap.contains("duplicate audit row"))
    );
}
