// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Tests for merging run outputs into the census files.

use std::collections::BTreeMap;
use std::fs;

use super::*;
use crate::model::{Cell, CellRow, Kind, StandardEntry, SymlistSummary, SymlistsOutput};
use crate::testutil::{TempDir, fixture};

fn cell(d: u32, h: u32) -> Cell {
    Cell { d, h }
}

fn sample_cells() -> Vec<CellRow> {
    let donors = [
        "glibc", "musl", "freebsd", "netbsd", "openbsd", "bionic", "illumos", "mlibc", "picolibc",
    ];
    let strlcpy: [Cell; 9] = [
        cell(1, 1),
        cell(1, 1),
        cell(3, 2),
        cell(3, 2),
        cell(1, 1),
        cell(0, 2),
        cell(1, 3),
        cell(2, 2),
        cell(1, 1),
    ];
    let epoll: [Cell; 9] = [
        cell(1, 1),
        cell(1, 1),
        cell(0, 0),
        cell(1, 1),
        cell(0, 0),
        cell(1, 1),
        cell(1, 1),
        cell(1, 1),
        cell(0, 0),
    ];
    let mut rows = Vec::new();
    for (name, values, n6, families, libcs) in [
        (
            "strlcpy",
            strlcpy,
            6,
            vec!["bsd", "linux", "solaris"],
            vec![
                "bionic", "freebsd", "glibc", "illumos", "mlibc", "musl", "netbsd", "openbsd",
                "picolibc",
            ],
        ),
        (
            "epoll_create",
            epoll,
            4,
            vec!["bsd", "linux", "solaris"],
            vec!["bionic", "glibc", "illumos", "mlibc", "musl", "netbsd"],
        ),
    ] {
        let mut cells = BTreeMap::new();
        for (donor, value) in donors.iter().zip(values) {
            cells.insert((*donor).to_string(), value);
        }
        rows.push(CellRow {
            name: name.to_string(),
            kind: Kind::Func,
            cells,
            n6,
            libcs: libcs.into_iter().map(str::to_string).collect(),
            families: families.into_iter().map(str::to_string).collect(),
        });
    }
    rows
}

fn sample_standards() -> StandardsOutput {
    let mut entries = BTreeMap::new();
    entries.insert(
        "strlcpy".to_string(),
        StandardEntry {
            standard: vec!["posix-2024".to_string()],
            header: Some("string.h".to_string()),
            kind: "func".to_string(),
        },
    );
    StandardsOutput { entries }
}

fn sample_summary(complete: bool) -> RunSummary {
    RunSummary {
        census_rev: "test-rev".to_string(),
        complete,
        donors: BTreeMap::from([
            ("bionic".to_string(), "bbb".to_string()),
            ("glibc".to_string(), "aaa".to_string()),
        ]),
        ..RunSummary::default()
    }
}

fn sample_assignments() -> Assignments {
    Assignments {
        schema: Some(1),
        assign: vec![
            Assignment {
                name: "strlcpy".to_string(),
                ring: Some(Ring::Number(1)),
                ring_override: Some("P24? row X01, reviewer decides promotion".to_string()),
                crate_name: Some("str".to_string()),
                area: Some(1),
                status: Some("EXISTS-donor".to_string()),
                ..Assignment::default()
            },
            Assignment {
                name: "epoll_create".to_string(),
                ring: Some(Ring::Number(2)),
                personality: Some("linux".to_string()),
                ring_override: Some(
                    "doc 02 R-02-2: single-family facility in the brief, personality linux"
                        .to_string(),
                ),
                header: Some("sys/epoll.h".to_string()),
                crate_name: Some("personality-linux".to_string()),
                area: Some(11),
                status: Some("TO-BE-DESIGNED".to_string()),
                ..Assignment::default()
            },
        ],
    }
}

struct Prepared {
    temp: TempDir,
    options: MergeOptions,
}

fn prepare(label: &str, complete: bool) -> Prepared {
    let temp = TempDir::new(label);
    let out = temp.path().join("out");
    let census = temp.path().join("census");
    fs::create_dir_all(&out).unwrap();
    fs::write(
        out.join("cells.json"),
        serde_json::to_string(&sample_cells()).unwrap(),
    )
    .unwrap();
    fs::write(
        out.join("standards.json"),
        serde_json::to_string(&sample_standards()).unwrap(),
    )
    .unwrap();
    fs::write(
        out.join("run-summary.json"),
        serde_json::to_string(&sample_summary(complete)).unwrap(),
    )
    .unwrap();
    fs::create_dir_all(&census).unwrap();
    fs::write(
        census.join("assignments.toml"),
        toml::to_string(&sample_assignments()).unwrap(),
    )
    .unwrap();
    Prepared {
        temp,
        options: MergeOptions {
            out,
            census,
            assignments: None,
        },
    }
}

#[test]
fn symbols_toml_has_a_provenance_header_and_sorted_rows() {
    let prepared = prepare("merge-header", false);
    let report = merge(&prepared.options).unwrap();
    assert_eq!(report.symbols, 2);
    let text = fs::read_to_string(prepared.options.census.join("symbols.toml")).unwrap();
    assert!(text.starts_with("# SPDX-License-Identifier: BSD-3-Clause\n"));
    assert!(text.contains("# GENERATED by libc-census; do not edit"));
    assert!(text.contains("schema = 1"));
    assert!(text.contains("census_rev = \"test-rev\""));
    assert!(text.contains("complete = false"));
    assert!(text.contains("[donors]"));
    assert!(text.contains("glibc = \"aaa\""));
    let epoll = text.find("name = \"epoll_create\"").unwrap();
    let strlcpy = text.find("name = \"strlcpy\"").unwrap();
    assert!(epoll < strlcpy);
    assert_eq!(text.matches("[[symbol]]").count(), 2);
}

#[test]
fn golden_symbols_and_census_match_the_fixture() {
    let prepared = prepare("merge-golden", false);
    merge(&prepared.options).unwrap();
    let symbols = fs::read_to_string(prepared.options.census.join("symbols.toml")).unwrap();
    let expected = fs::read_to_string(fixture("expected/symbols.toml")).unwrap();
    assert_eq!(symbols, expected, "symbols.toml golden");
    let census = fs::read_to_string(prepared.options.census.join("census.md")).unwrap();
    let expected = fs::read_to_string(fixture("expected/census.md")).unwrap();
    assert_eq!(census, expected, "census.md golden");
}

#[test]
fn a_differing_assignment_is_recorded_in_ring_diff() {
    let prepared = prepare("merge-ringdiff", false);
    let report = merge(&prepared.options).unwrap();
    assert_eq!(report.ring_diff, 2);
    let text = fs::read_to_string(prepared.options.census.join("ring_diff.md")).unwrap();
    assert!(text.contains("`strlcpy` | 0 | 1 | P24?"));
    assert!(text.contains("`epoll_create` | 1 | 2 |"));
}

#[test]
fn a_differing_assignment_without_a_reason_is_an_error() {
    let prepared = prepare("merge-noreason", false);
    let mut assignments = sample_assignments();
    assignments.assign[0].ring_override = None;
    fs::write(
        prepared.options.census.join("assignments.toml"),
        toml::to_string(&assignments).unwrap(),
    )
    .unwrap();
    let error = merge(&prepared.options).unwrap_err().to_string();
    assert!(error.contains("ring_override"), "{error}");
}

#[test]
fn a_partial_run_writes_complete_false() {
    let prepared = prepare("merge-partial", false);
    let report = merge(&prepared.options).unwrap();
    assert!(!report.complete);
    let text = fs::read_to_string(prepared.options.census.join("symbols.toml")).unwrap();
    assert!(text.contains("complete = false"));
}

#[test]
fn an_empty_out_directory_gives_an_empty_symbols_file() {
    let temp = TempDir::new("merge-empty");
    let out = temp.path().join("out");
    let census = temp.path().join("census");
    fs::create_dir_all(&out).unwrap();
    let report = merge(&MergeOptions {
        out,
        census: census.clone(),
        assignments: None,
    })
    .unwrap();
    assert_eq!(report.symbols, 0);
    let text = fs::read_to_string(census.join("symbols.toml")).unwrap();
    assert!(text.contains("schema = 1"));
    assert!(!text.contains("[[symbol]]"));
}

#[test]
fn an_assignment_only_name_gets_a_row() {
    let prepared = prepare("merge-assignonly", false);
    let mut assignments = sample_assignments();
    assignments.assign.push(Assignment {
        name: "gets".to_string(),
        ring: Some(Ring::Number(3)),
        area: Some(15),
        status: Some("SKIP".to_string()),
        stub: Some("reads-to-newline".to_string()),
        ..Assignment::default()
    });
    fs::write(
        prepared.options.census.join("assignments.toml"),
        toml::to_string(&assignments).unwrap(),
    )
    .unwrap();
    let report = merge(&prepared.options).unwrap();
    let row = report.rows.iter().find(|row| row.name == "gets").unwrap();
    assert_eq!(row.ring, Some(Ring::Number(3)));
    assert_eq!(row.ring_computed, 3);
    assert!(row.libcs.is_empty());
}

#[test]
fn two_merges_produce_identical_bytes() {
    let prepared = prepare("merge-determinism", false);
    merge(&prepared.options).unwrap();
    let first = read_census(&prepared.options.census);
    let second_dir = prepared.temp.path().join("census2");
    merge(&MergeOptions {
        out: prepared.options.out.clone(),
        census: second_dir.clone(),
        assignments: Some(prepared.options.census.join("assignments.toml")),
    })
    .unwrap();
    let second = read_census(&second_dir);
    assert_eq!(first, second);
}

fn read_census(census: &Path) -> BTreeMap<String, Vec<u8>> {
    let mut files = BTreeMap::new();
    for name in [
        "symbols.toml",
        "census.json",
        "census.md",
        "ring_diff.md",
        "patchset-gaps.md",
        "known-differences.toml",
    ] {
        files.insert(
            name.to_string(),
            fs::read(census.join(name)).unwrap_or_default(),
        );
    }
    files
}

#[test]
fn known_differences_is_written_once_and_never_overwritten() {
    let prepared = prepare("merge-known", false);
    fs::write(
        prepared.options.census.join("known-differences.toml"),
        "# reviewer text\nschema = 1\n",
    )
    .unwrap();
    merge(&prepared.options).unwrap();
    let text = fs::read_to_string(prepared.options.census.join("known-differences.toml")).unwrap();
    assert_eq!(text, "# reviewer text\nschema = 1\n");
}

#[test]
fn census_json_has_the_ring_and_n6_histograms() {
    let prepared = prepare("merge-census", false);
    merge(&prepared.options).unwrap();
    let text = fs::read_to_string(prepared.options.census.join("census.json")).unwrap();
    let census: Census = serde_json::from_str(&text).unwrap();
    assert_eq!(census.rings["0"], 0);
    assert_eq!(census.rings["1"], 1);
    assert_eq!(census.rings["2"], 1);
    assert_eq!(census.n6["4"], 1);
    assert_eq!(census.n6["6"], 1);
    assert_eq!(census.ring1_candidates, vec!["epoll_create"]);
}

#[test]
fn patchset_gaps_records_a_skipped_snapshot() {
    let prepared = prepare("merge-gaps", false);
    merge(&prepared.options).unwrap();
    let text = fs::read_to_string(prepared.options.census.join("patchset-gaps.md")).unwrap();
    assert!(text.contains("skipped: no ports snapshot given"));
}

#[test]
fn symlists_output_is_not_required_by_merge() {
    let prepared = prepare("merge-symlists", false);
    let extra = SymlistsOutput {
        sources: BTreeMap::from([(
            "openbsd".to_string(),
            SymlistSummary {
                count: 1,
                names: vec!["strlcpy".to_string()],
            },
        )]),
    };
    fs::write(
        prepared.options.out.join("symlists.json"),
        serde_json::to_string(&extra).unwrap(),
    )
    .unwrap();
    assert_eq!(merge(&prepared.options).unwrap().symbols, 2);
}
