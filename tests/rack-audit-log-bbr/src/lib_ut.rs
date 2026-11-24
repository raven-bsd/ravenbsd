// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Exact audit checks for the K328/K329 merged slice.

use super::{BBR_ROWS, DONOR_CONSTANTS, DONOR_DEFAULTS, LOG_ROWS, matrix};

const BBR_SRC: &str = include_str!("../../../crates/raven-tcpip/src/rack_bbr.rs");
const BBR_UT: &str = include_str!("../../../crates/raven-tcpip/src/rack_bbr_ut.rs");
const LOG_SRC: &str = include_str!("../../../crates/raven-tcpip/src/rack_log.rs");
const LOG_UT: &str = include_str!("../../../crates/raven-tcpip/src/rack_log_ut.rs");
const OS_SRC: &str = include_str!("../../../crates/raven-tcpip/src/rack_os.rs");
const OS_UT: &str = include_str!("../../../crates/raven-tcpip/src/rack_os_ut.rs");

fn method(mapping: &str) -> &str {
    mapping
        .rsplit("::")
        .next()
        .unwrap_or(mapping)
        .rsplit('.')
        .next()
        .unwrap_or(mapping)
}

fn tests_source() -> String {
    format!("{BBR_UT}\n{LOG_UT}\n{OS_UT}")
}

#[test]
fn audit_table_has_78_unique_rows() {
    let rows: Vec<_> = matrix().collect();
    assert_eq!(rows.len(), 78);
    assert_eq!(BBR_ROWS.len(), 35);
    assert_eq!(LOG_ROWS.len(), 43);
    let mut names: Vec<&str> = rows.iter().map(|row| row.0).collect();
    names.sort_unstable();
    names.dedup();
    assert_eq!(names.len(), 78, "donor function appears more than once");
}

#[test]
fn every_mapping_and_named_test_exists() {
    let tests = tests_source();
    let mut missing = Vec::new();
    for (function, mapping, test, mutation) in matrix() {
        assert!(
            !test.is_empty() && !mutation.is_empty(),
            "{function}: blank row"
        );
        let source = if mapping.starts_with("RackOs") {
            OS_SRC
        } else if mapping.starts_with("crate::rack_log") {
            LOG_SRC
        } else {
            BBR_SRC
        };
        if !source.contains(&format!("fn {}", method(mapping))) {
            missing.push(format!("{function}: missing {}", method(mapping)));
        }
        if !tests.contains(test) {
            missing.push(format!("{function}: missing test {test}"));
        }
    }
    assert!(missing.is_empty(), "{missing:#?}");
}

#[test]
fn donor_constants_match_the_pinned_header() {
    let mut missing = Vec::new();
    for constant in DONOR_CONSTANTS {
        if !BBR_SRC.contains(constant) {
            missing.push(*constant);
        }
    }
    for (needle, donor) in DONOR_DEFAULTS {
        if !BBR_SRC.contains(needle) {
            missing.push(needle);
            missing.push(donor);
        }
    }
    assert!(missing.is_empty(), "donor constant mismatch: {missing:#?}");
}

#[test]
fn known_discrepancies_are_exactly_reported() {
    // K328's matrix claims four K314 RackOs functions. They exist, but they are
    // context rows, not K328-owned donor functions.
    let foreign: Vec<&str> = BBR_ROWS
        .iter()
        .filter(|row| row.1.starts_with("RackOs::"))
        .map(|row| row.0)
        .collect();
    assert_eq!(
        foreign,
        vec![
            "rack_set_cc_pacing",
            "rack_undo_cc_pacing",
            "rack_remove_pacing",
            "rack_get_pacing_len"
        ]
    );

    // BbrRack keeps a second logging type for eight donor logging functions
    // already implemented by K329 `RackLog`. The donor function is logged in a
    // different struct; this is the duplicate-type finding for the group owner.
    let mut duplicate_logs = Vec::new();
    for (function, _, _, _) in LOG_ROWS {
        if let Some(suffix) = function.strip_prefix("rack_log_")
            && BBR_SRC.contains(&format!("fn log_{suffix}("))
        {
            duplicate_logs.push(*function);
        }
    }
    assert_eq!(
        duplicate_logs,
        vec![
            "rack_log_gpset",
            "rack_log_hybrid_bw",
            "rack_log_hdwr_pacing",
            "rack_log_type_pacing_sizes",
            "rack_log_timely",
            "rack_log_gp_calc",
            "rack_log_hybrid",
            "rack_log_pacing_delay_calc",
        ]
    );

    // Five K334-owned donor functions are implemented as private BBR helpers but
    // are not claimed by K328's 35-row matrix. They are not duplicates of a
    // merged K334 implementation yet, but they are unclaimed donor functions.
    let mut unclaimed = Vec::new();
    for function in [
        "rack_get_decrease",
        "rack_decrease_highrtt",
        "rack_set_prtt_target",
        "rack_set_pace_segments",
        "rack_set_dgp",
    ] {
        let method = function.strip_prefix("rack_").unwrap_or(function);
        if BBR_SRC.contains(&format!("fn {method}(")) {
            unclaimed.push(function);
        }
    }
    assert_eq!(
        unclaimed,
        vec![
            "rack_get_decrease",
            "rack_decrease_highrtt",
            "rack_set_prtt_target",
            "rack_set_pace_segments",
            "rack_set_dgp",
        ]
    );
}

#[test]
fn all_rows_have_a_named_mutation() {
    for (function, _, _, mutation) in matrix() {
        assert!(
            !mutation.is_empty() && mutation != "-",
            "{function}: mutation is not named"
        );
    }
}

#[test]
fn matrix_print_shape_is_checked_by_the_binary() {
    // The binary writes the K315 matrix; keep the row count stable for it.
    assert_eq!(matrix().count(), 78);
}
