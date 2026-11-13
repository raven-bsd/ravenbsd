// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Tests for `compare-table`.

use std::fs;

use super::*;
use crate::testutil::{TempDir, fixture};

fn table(rows: &str) -> String {
    format!(
        "#### 3.B.4 The census table\n\n\
         | Function | glibc | musl | FBSD | NBSD | OBSD | bionic | illumos | mlibc | picolibc | n6 | Ring | Action |\n\
         |---|---|---|---|---|---|---|---|---|---|---|---|---|\n\
         {rows}\n\
         #### 3.B.5 Licence classes\n"
    )
}

fn options(temp: &TempDir, table: &Path) -> CompareOptions {
    CompareOptions {
        table: table.to_path_buf(),
        resources: fixture("resources"),
        require_all: false,
        known: temp.path().join("known-differences.toml"),
        out: temp.path().join("out"),
    }
}

fn write_table(temp: &TempDir, text: &str) -> std::path::PathBuf {
    temp.write("table.md", text);
    temp.path().join("table.md")
}

#[test]
fn the_excerpt_has_the_expected_rows() {
    let table = parse_table(&fixture("table15-excerpt.md")).unwrap();
    assert_eq!(table.symbol_rows(), 4);
    assert_eq!(table.header_rows(), 2);
    assert_eq!(table.rows[0].key, "strlcpy");
    assert_eq!(table.rows[1].key, "epoll_create");
    assert!(table.rows[1].cells[0] == "1/1");
    assert!(table.rows[2].data);
    assert_eq!(table.rows[2].key, "environ");
    assert_eq!(table.rows[4].key, "<string.h>");
    assert_eq!(table.rows[5].header.as_deref(), Some("sys/queue.h"));
}

#[test]
fn annotated_cells_use_the_leading_d_over_h() {
    assert_eq!(parse_symbol_cell("1/1each"), Some((1, 1)));
    assert_eq!(parse_symbol_cell("**1/1**"), Some((1, 1)));
    assert_eq!(parse_symbol_cell("1/1(unvis2/2)"), Some((1, 1)));
    assert_eq!(parse_symbol_cell("0/2"), Some((0, 2)));
    assert_eq!(parse_symbol_cell("yes"), None);
}

#[test]
fn the_excerpt_passes_against_the_fixture_resources() {
    let temp = TempDir::new("table-pass");
    let report = compare(&options(&temp, &fixture("table15-excerpt.md"))).unwrap();
    assert_eq!(report.status, Exit::Ok, "{report:?}");
    assert!(report.symbol_differences.is_empty(), "{report:?}");
    assert!(report.header_differences.is_empty(), "{report:?}");
    assert_eq!(report.present_donors.len(), 9);
    assert!(!report.partial);
    let text = temp.read("out/compare-table.md");
    assert!(text.contains("Result: PASS"));
}

#[test]
fn a_flipped_symbol_cell_fails_and_names_the_cell() {
    let temp = TempDir::new("table-flip");
    let text = table(
        "| `strlcpy` | 1/1 | 1/1 | 1/1 | 1/1 | 2/2 | 0/1 | 1/1 | 1/1 | 1/1 | 6 | 1 | fixture |",
    );
    let path = write_table(&temp, &text);
    let report = compare(&options(&temp, &path)).unwrap();
    assert_eq!(report.status, Exit::GateFailed);
    assert_eq!(report.symbol_differences.len(), 1);
    let difference = &report.symbol_differences[0];
    assert_eq!(difference.key, "strlcpy");
    assert_eq!(difference.donor, "openbsd");
    assert_eq!(difference.doc, "2/2");
    assert_eq!(difference.tool, "1/1");
    let text = temp.read("out/compare-table.md");
    assert!(text.contains("`strlcpy`"));
    assert!(text.contains("openbsd"));
    assert!(text.contains("2/2"));
}

#[test]
fn a_header_difference_must_be_listed() {
    let temp = TempDir::new("table-header");
    let text = table(
        "| `<sys/queue.h>` | no | no | yes | yes | yes | yes | yes | no | no | 4 | 1 | fixture |",
    );
    let path = write_table(&temp, &text);
    let report = compare(&options(&temp, &path)).unwrap();
    assert_eq!(report.status, Exit::GateFailed);
    assert_eq!(report.header_differences.len(), 1);
    assert_eq!(report.header_differences[0].donor, "bionic");

    temp.write(
        "known-differences.toml",
        "[[difference]]\nkey = \"<sys/queue.h>\"\ndonor = \"bionic\"\ndoc = \"yes\"\ntool = \"no\"\nreason = \"fixture annotation\"\n",
    );
    let report = compare(&options(&temp, &path)).unwrap();
    assert_eq!(report.status, Exit::Ok);
    let text = temp.read("out/compare-table.md");
    assert!(text.contains("Result: PASS"));
}

#[test]
fn more_than_five_header_differences_fail_even_when_listed() {
    let temp = TempDir::new("table-header-limit");
    let mut rows = String::new();
    for header in [
        "fixture-one.h",
        "fixture-two.h",
        "fixture-three.h",
        "fixture-four.h",
        "fixture-five.h",
        "fixture-six.h",
    ] {
        rows.push_str(&format!(
            "| `<{header}>` | yes | no | no | no | no | no | no | no | no | 0 | 0 | fixture |\n"
        ));
    }
    let path = write_table(&temp, &table(&rows));
    let mut known = String::new();
    for header in [
        "fixture-one.h",
        "fixture-two.h",
        "fixture-three.h",
        "fixture-four.h",
        "fixture-five.h",
        "fixture-six.h",
    ] {
        known.push_str(&format!(
            "[[difference]]\nkey = \"<{header}>\"\ndonor = \"glibc\"\ndoc = \"no\"\ntool = \"yes\"\nreason = \"fixture\"\n"
        ));
    }
    temp.write("known-differences.toml", &known);
    let report = compare(&options(&temp, &path)).unwrap();
    assert_eq!(report.header_differences.len(), 6);
    assert_eq!(report.status, Exit::GateFailed);
}

#[test]
fn a_variable_row_uses_the_variable_regex() {
    let temp = TempDir::new("table-var");
    let text = table(
        "| `environ` (var) | 1/0 | 0/0 | 0/0 | 0/0 | 0/0 | 0/0 | 0/0 | 0/0 | 0/0 | 1 | 1 | fixture |",
    );
    let path = write_table(&temp, &text);
    let report = compare(&options(&temp, &path)).unwrap();
    assert_eq!(report.status, Exit::Ok, "{report:?}");
}

#[test]
fn require_all_turns_a_missing_donor_into_exit_two() {
    let temp = TempDir::new("table-partial");
    temp.write(
        "resources/glibc/include/string.h",
        "extern int strlcpy(void);\n",
    );
    let mut options = options(&temp, &fixture("table15-excerpt.md"));
    options.resources = temp.path().join("resources");
    let report = compare(&options).unwrap();
    assert!(report.partial);
    assert!(report.missing_donors.contains(&"openbsd".to_string()));
    assert_eq!(report.status, Exit::GateFailed);
    options.require_all = true;
    let report = compare(&options).unwrap();
    assert_eq!(report.status, Exit::Usage);
}

#[test]
fn a_missing_section_is_an_error() {
    let temp = TempDir::new("table-nosection");
    temp.write("table.md", "nothing here\n");
    let result = parse_table(&temp.path().join("table.md"));
    assert!(result.is_err());
}

#[test]
fn a_symbol_row_without_a_cell_is_recorded_as_a_parse_error() {
    let temp = TempDir::new("table-badcell");
    let text =
        table("| `strlcpy` | yes | 1/1 | 1/1 | 1/1 | 1/1 | 0/1 | 1/1 | 1/1 | 1/1 | 6 | 1 | x |");
    let path = write_table(&temp, &text);
    let report = compare(&options(&temp, &path)).unwrap();
    assert!(!report.parse_errors.is_empty());
    assert_eq!(report.status, Exit::GateFailed);
}

#[test]
fn the_report_is_written_even_when_the_gate_fails() {
    let temp = TempDir::new("table-report");
    let text = table(
        "| `strlcpy` | 2/2 | 1/1 | 1/1 | 1/1 | 1/1 | 0/1 | 1/1 | 1/1 | 1/1 | 6 | 1 | fixture |",
    );
    let path = write_table(&temp, &text);
    compare(&options(&temp, &path)).unwrap();
    let report = fs::read_to_string(temp.path().join("out/compare-table.md")).unwrap();
    assert!(report.contains("Result: FAIL"));
    assert!(report.contains("Symbol differences: 1"));
}
