// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Audit checks for the merged K334/K335 RACK misc/transport group.

use std::collections::BTreeSet;

use super::*;

const MISC: &str = include_str!("../../../crates/raven-tcpip/src/rack_misc.rs");
const TRANSPORT: &str = include_str!("../../../crates/raven-tcpip/src/rack_transport.rs");
const MISC_TESTS: &str = include_str!("../../../crates/raven-tcpip/tests/rack_misc.rs");
const TRANSPORT_UT: &str = include_str!("../../../crates/raven-tcpip/src/rack_transport_ut.rs");
const TRANSPORT_TESTS: &str = include_str!("../../../crates/raven-tcpip/tests/rack_transport.rs");
const MANIFEST: &str = include_str!("../../../tests/rack-coverage/fixtures/rack-functions.txt");
const GROUP: &str = include_str!("../fixtures/group.tsv");
const CONSTANTS: &str = include_str!("../fixtures/constants.tsv");

struct GroupRow {
    owner: &'static str,
    function: &'static str,
    rust_path: &'static str,
    test: &'static str,
    mutation_status: &'static str,
}

struct ConstantRow {
    name: &'static str,
    donor: Option<u64>,
    rust_file: &'static str,
    expected: &'static str,
}

fn group_rows() -> Vec<GroupRow> {
    GROUP
        .lines()
        .filter(|line| !line.starts_with('#') && !line.starts_with("owner\t"))
        .map(|line| {
            let cells: Vec<&str> = line.split('\t').collect();
            assert_eq!(cells.len(), 6, "bad group fixture row: {line}");
            GroupRow {
                owner: cells[0],
                function: cells[1],
                rust_path: cells[2],
                test: cells[3],
                mutation_status: cells[4],
            }
        })
        .collect()
}

fn constant_rows() -> Vec<ConstantRow> {
    CONSTANTS
        .lines()
        .filter(|line| !line.starts_with('#') && !line.starts_with("name\t"))
        .map(|line| {
            let cells: Vec<&str> = line.split('\t').collect();
            assert_eq!(cells.len(), 4, "bad constant fixture row: {line}");
            let donor = if cells[1] == "-" {
                None
            } else {
                Some(cells[1].parse().expect("donor integer"))
            };
            ConstantRow {
                name: cells[0],
                donor,
                rust_file: cells[2],
                expected: cells[3],
            }
        })
        .collect()
}

fn const_value(source: &str, name: &str) -> Option<u64> {
    let marker = format!("pub const {name}:");
    let line = source.lines().find(|line| line.starts_with(&marker))?;
    let value = line.split_once('=')?.1.trim().trim_end_matches(';').trim();
    let value = value.replace('_', "");
    if let Some(hex) = value.strip_prefix("0x") {
        u64::from_str_radix(hex, 16).ok()
    } else {
        value.parse().ok()
    }
}

fn source_for(rust_file: &str) -> &'static str {
    match rust_file {
        "misc" => MISC,
        "transport" => TRANSPORT,
        other => panic!("unknown rust file {other}"),
    }
}

fn test_source_for(owner: &str) -> String {
    match owner {
        "K334" => MISC_TESTS.to_owned(),
        "K335" => format!("{TRANSPORT_UT}\n{TRANSPORT_TESTS}"),
        other => panic!("unknown owner {other}"),
    }
}

fn test_count(source: &str) -> usize {
    source
        .lines()
        .filter(|line| line.trim() == "#[test]")
        .count()
}

fn manifest_names() -> BTreeSet<&'static str> {
    MANIFEST
        .lines()
        .filter(|line| !line.starts_with('#') && !line.trim().is_empty())
        .filter_map(|line| line.split_whitespace().nth(1))
        .collect()
}

#[test]
fn fixture_has_the_expected_group_rows() {
    let rows = group_rows();
    assert_eq!(rows.len(), K334_ROWS + K335_ROWS);
    assert_eq!(
        rows.iter().filter(|row| row.owner == "K334").count(),
        K334_ROWS
    );
    assert_eq!(
        rows.iter().filter(|row| row.owner == "K335").count(),
        K335_ROWS
    );
}

#[test]
fn each_group_function_exists_in_the_merged_source() {
    for row in group_rows() {
        let source = source_for(if row.owner == "K334" {
            "misc"
        } else {
            "transport"
        });
        assert!(
            source.contains(&format!("fn {}(", row.function)),
            "{} is missing from the merged source",
            row.function
        );
        assert!(!row.rust_path.is_empty());
    }
}

#[test]
fn each_group_function_has_a_named_test() {
    for row in group_rows() {
        let tests = test_source_for(row.owner);
        assert!(
            tests.contains(&format!("fn {}(", row.test)),
            "{} has no named test {}",
            row.function,
            row.test
        );
    }
}

#[test]
fn all_group_functions_are_in_the_k315_manifest() {
    let names = manifest_names();
    assert_eq!(names.len(), 219);
    for row in group_rows() {
        assert!(
            names.contains(row.function),
            "{} is absent from the K315 donor manifest",
            row.function
        );
    }
}

#[test]
fn group_has_exactly_one_known_duplicate() {
    let rows = group_rows();
    let mut counts = BTreeSet::new();
    let mut duplicates = BTreeSet::new();
    for row in &rows {
        if !counts.insert(row.function) {
            duplicates.insert(row.function);
        }
    }
    assert_eq!(counts.len(), UNIQUE_GROUP_FUNCTIONS);
    assert_eq!(rows.len(), 37);
    assert_eq!(duplicates, BTreeSet::from(["rack_pru_options"]));
}

#[test]
fn the_duplicate_is_implemented_in_both_group_files() {
    assert!(MISC.contains("fn rack_pru_options("));
    assert!(TRANSPORT.contains("fn rack_pru_options("));
}

#[test]
fn k334_does_not_claim_the_k332_process_option_row() {
    assert!(
        !group_rows()
            .iter()
            .any(|row| row.owner == "K334" && row.function == "rack_process_option")
    );
    assert!(MISC.contains("fn rack_process_option("));
}

#[test]
pub fn k334_mutation_evidence_is_a_recorded_gap() {
    let rows: Vec<GroupRow> = group_rows()
        .into_iter()
        .filter(|row| row.owner == "K334")
        .collect();
    // K334's report gives one constant mutation, not one per donor row. The
    // audit records the gap rather than treating the claim as evidence.
    assert_eq!(rows.len(), K334_ROWS);
    assert!(rows.iter().all(|row| row.mutation_status == "GAP"));
}

#[test]
pub fn k335_mutations_are_recorded() {
    let rows: Vec<GroupRow> = group_rows()
        .into_iter()
        .filter(|row| row.owner == "K335")
        .collect();
    assert_eq!(rows.len(), K335_ROWS);
    assert!(rows.iter().all(|row| row.mutation_status == "PASS"));
}

#[test]
fn constants_match_or_are_the_known_findings() {
    let rows = constant_rows();
    let mut mismatches = BTreeSet::new();
    let mut invented = BTreeSet::new();
    for row in &rows {
        let actual = const_value(source_for(row.rust_file), row.name)
            .unwrap_or_else(|| panic!("{} is missing from {}", row.name, row.rust_file));
        match row.expected {
            "MATCH" => {
                assert_eq!(Some(actual), row.donor, "{} value changed", row.name);
            }
            "MISMATCH" => {
                assert_ne!(Some(actual), row.donor, "{} unexpectedly matches", row.name);
                mismatches.insert(row.name);
            }
            "INVENTED" => {
                assert!(row.donor.is_none());
                assert_eq!(actual, 16);
                invented.insert(row.name);
            }
            other => panic!("unknown expectation {other}"),
        }
    }
    assert_eq!(
        mismatches,
        BTreeSet::from(["RACK_FREE_CACHE", "RACK_FREE_CNT_MAX"])
    );
    assert_eq!(invented, BTreeSet::from(["RACK_DEFERRED_MAX"]));
}

#[test]
fn donor_revision_is_pinned() {
    assert_eq!(DONOR_REV.len(), 40);
    assert!(DONOR_REV.starts_with("133344405f"));
}

#[test]
fn group_test_counts_match_the_merged_reports() {
    assert_eq!(test_count(MISC_TESTS), 28);
    assert_eq!(test_count(TRANSPORT_UT), 13);
    assert_eq!(test_count(TRANSPORT_TESTS), 2);
}
