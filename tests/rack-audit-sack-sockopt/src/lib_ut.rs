// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Unit tests and negative fixtures for the K356 audit itself.

use super::*;

const SACK_OK: &str = "
pub const RACK_ACKED: u32 = 0x01;
pub const RACK_SACK_PASSED: u32 = 0x10;
pub const RACK_WAS_SACKPASS: u32 = 0x20;
pub const RACK_WAS_LOST: u32 = 0x400000;
pub const RACK_RWND_COLLAPSED: u32 = 0x100;
";

const MISC_OK: &str = "
pub const RACK_ACKED: u32 = 0x01;
pub const RACK_SACK_PASSED: u32 = 0x10;
";

const TESTS_OK: &str = "
fn rack_log_dsack_event_x() {}
fn rack_log_sack_passed_x() {}
fn rack_proc_sack_blk_x() {}
fn rack_peer_reneges_x() {}
fn rack_process_to_cumack_x() {}
fn rack_note_dsack_x() {}
fn rack_handle_might_revert_x() {}
";

#[test]
fn wrong_constant_is_detected_and_correct_one_is_accepted() {
    let findings = audit_sack(SACK_OK, MISC_OK, TESTS_OK);
    assert!(
        !findings
            .iter()
            .any(|f| f.code == "SACK-WRONG-CONSTANT" && f.subject == "RACK_SACK_PASSED")
    );
    let mutated = SACK_OK.replace(
        "RACK_SACK_PASSED: u32 = 0x10",
        "RACK_SACK_PASSED: u32 = 0x02",
    );
    let findings = audit_sack(&mutated, MISC_OK, TESTS_OK);
    assert!(findings.iter().any(|f| {
        f.code == "SACK-WRONG-CONSTANT"
            && f.subject == "RACK_SACK_PASSED"
            && f.detail.contains("0x000002")
            && f.detail.contains("0x000010")
    }));
}

#[test]
fn duplicate_constant_and_owner_overlap_are_detected() {
    let findings = audit_sack(SACK_OK, MISC_OK, TESTS_OK);
    assert!(findings.iter().any(|f| f.code == "SACK-DUPLICATE-CONSTANT"));
    assert!(findings.iter().any(|f| {
        f.code == "SACK-DUPLICATE-OWNER"
            && matches!(
                f.subject.as_str(),
                "rack_log_dsack_event" | "rack_log_sack_passed" | "rack_process_to_cumack"
            )
    }));
    assert!(
        findings
            .iter()
            .any(|f| f.code == "SACK-MISSING-OWNER" && f.subject == "rack_handle_might_revert")
    );
    assert!(
        !findings
            .iter()
            .any(|f| f.code == "SACK-MISSING-TEST" && f.subject == "rack_proc_sack_blk")
    );
}

#[test]
fn missing_test_is_detected() {
    let no_tests = TESTS_OK.replace("fn rack_note_dsack_x() {}\n", "");
    let findings = audit_sack(SACK_OK, MISC_OK, &no_tests);
    assert!(
        findings
            .iter()
            .any(|f| f.code == "SACK-MISSING-TEST" && f.subject == "rack_note_dsack")
    );
}

#[test]
fn sockopt_audit_reports_the_known_donor_divergences() {
    let findings = audit_sockopt();
    for (code, subject) in [
        ("SOCKOPT-DONOR-DIVERGENCE", "TCP_RACK_MEASURE_CNT=0"),
        ("SOCKOPT-DONOR-DIVERGENCE", "TCP_RACK_ABC_VAL=300"),
        ("SOCKOPT-DONOR-DIVERGENCE", "TCP_RACK_PACE_MIN_SEG=70000"),
        ("SOCKOPT-DONOR-DIVERGENCE", "TCP_INFO get"),
        ("SOCKOPT-MISSING-OPTION", "TCP_RACK_PACING_BETA_ECN"),
    ] {
        assert!(
            findings
                .iter()
                .any(|f| f.code == code && f.subject == subject),
            "missing {code} {subject}: {findings:?}"
        );
    }
}
