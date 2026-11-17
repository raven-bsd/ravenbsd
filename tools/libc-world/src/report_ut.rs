// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

use std::collections::{BTreeMap, BTreeSet};

use super::*;
use crate::census_out::write_undef;
use crate::testutil::{TempDir, write};

const WORLD: &str = r#"
schema = 1
freeze = "pending"
weights = { optional = 0.5 }
[[package]]
n = 1
name = "zlib"
tier = 1
build = "cf"
version = "PLACEHOLDER"
url = "PLACEHOLDER"
sha256 = "PLACEHOLDER"
tarball = "PLACEHOLDER"
stresses = "stdio"
[[package]]
n = 2
name = "lua"
tier = 2
build = "mk"
version = "PLACEHOLDER"
url = "PLACEHOLDER"
sha256 = "PLACEHOLDER"
tarball = "PLACEHOLDER"
stresses = "strtod"
[[package]]
n = 3
name = "sqlite"
tier = 2
build = "ac"
version = "PLACEHOLDER"
url = "PLACEHOLDER"
sha256 = "PLACEHOLDER"
tarball = "PLACEHOLDER"
stresses = "mmap"
"#;

fn world() -> World {
    World::parse(WORLD).expect("world")
}

fn built(n: u32, name: &str) -> PackageResult {
    PackageResult::new(n, name, "1.0", "built")
}

#[test]
fn the_result_path_is_arch_and_name() {
    assert_eq!(
        package_result_path(Path::new("/r"), "x86_64", "zlib"),
        Path::new("/r/x86_64/zlib.json")
    );
}

#[test]
fn a_result_round_trips() {
    let temp = TempDir::new("report-round");
    let mut result = built(1, "zlib");
    result.libs = vec!["m".to_string()];
    let path = write_result(temp.path(), "x86_64", &result).expect("write");
    let results = read_results(&temp.join("x86_64")).expect("read");
    assert_eq!(results, vec![result]);
    assert!(path.is_file());
}

#[test]
fn results_are_sorted_by_row_number() {
    let temp = TempDir::new("report-sort");
    let directory = temp.join("x86_64");
    write_result(temp.path(), "x86_64", &built(3, "sqlite")).expect("write");
    write_result(temp.path(), "x86_64", &built(1, "zlib")).expect("write");
    let results = read_results(&directory).expect("read");
    assert_eq!(
        results.iter().map(|result| result.n).collect::<Vec<_>>(),
        vec![1, 3]
    );
}

#[test]
fn world_json_is_skipped_by_the_reader() {
    let temp = TempDir::new("report-world-json");
    let directory = temp.join("x86_64");
    std::fs::create_dir_all(&directory).expect("dir");
    write(&directory.join("world.json"), "[]\n");
    assert!(read_results(&directory).expect("read").is_empty());
}

#[test]
fn world_json_is_an_array_sorted_by_row_number() {
    let temp = TempDir::new("report-world-array");
    let results = vec![built(3, "sqlite"), built(1, "zlib")];
    write_world(temp.path(), "x86_64", &results, &world()).expect("world");
    let text = std::fs::read_to_string(temp.join("x86_64/world.json")).expect("read");
    assert!(text.starts_with('['));
    let parsed: Vec<PackageResult> = serde_json::from_str(&text).expect("parse");
    assert_eq!(
        parsed.iter().map(|result| result.n).collect::<Vec<_>>(),
        vec![1, 3]
    );
}

#[test]
fn world_md_counts_statuses() {
    let results = vec![
        built(1, "zlib"),
        PackageResult::new(2, "lua", "PLACEHOLDER", "placeholder"),
    ];
    let text = world_md(
        &results,
        &world(),
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
    );
    assert!(text.contains("| built | 1 |"));
    assert!(text.contains("| placeholder | 1 |"));
    assert!(text.contains("| failed | 0 |"));
}

#[test]
fn world_md_has_the_fixed_column_header() {
    let text = world_md(
        &[],
        &world(),
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
    );
    assert!(text.contains(
        "| n | package | tier | status | failure class | libc patches | platform patches | undefined symbols |"
    ));
}

#[test]
fn world_md_counts_weighted_builds() {
    let results = vec![built(1, "zlib"), built(2, "lua")];
    let text = world_md(
        &results,
        &world(),
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
    );
    assert!(text.contains("Weighted built: 2 of 3"));
}

#[test]
fn world_md_renders_a_package_row() {
    let results = vec![built(1, "zlib")];
    let text = world_md(
        &results,
        &world(),
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
    );
    assert!(text.contains("| 1 | zlib | 1 | built | - | 0 | 0 | 0 |"));
}

#[test]
fn missing_symbols_aggregate_across_packages() {
    let temp = TempDir::new("report-missing");
    let mut first = BTreeMap::new();
    first.insert("strlcpy".to_string(), 'U');
    first.insert("weak".to_string(), 'w');
    let mut second = BTreeMap::new();
    second.insert("strlcpy".to_string(), 'w');
    write_undef(temp.path(), "x86_64", "a", &first).expect("a");
    write_undef(temp.path(), "x86_64", "b", &second).expect("b");
    let aggregate = aggregate_missing(temp.path(), "x86_64").expect("aggregate");
    assert_eq!(aggregate["strlcpy"].packages, 2);
    assert_eq!(aggregate["strlcpy"].strong, 1);
    assert_eq!(aggregate["strlcpy"].weak, 1);
    assert_eq!(aggregate["weak"].packages, 1);
}

#[test]
fn headers_aggregate_across_packages() {
    let temp = TempDir::new("report-headers");
    let mut first = BTreeSet::new();
    first.insert("stdio.h".to_string());
    first.insert("sys/cdefs.h".to_string());
    write_headers(temp.path(), "x86_64", "a", &first).expect("a");
    let mut second = BTreeSet::new();
    second.insert("stdio.h".to_string());
    write_headers(temp.path(), "x86_64", "b", &second).expect("b");
    let aggregate = aggregate_headers(temp.path(), "x86_64").expect("aggregate");
    assert_eq!(aggregate["stdio.h"], 2);
    assert_eq!(aggregate["sys/cdefs.h"], 1);
}

#[test]
fn libs_aggregate_across_results() {
    let mut first = built(1, "zlib");
    first.libs = vec!["m".to_string(), "z".to_string()];
    let mut second = built(2, "lua");
    second.libs = vec!["m".to_string()];
    let aggregate = aggregate_libs(&[first, second]);
    assert_eq!(aggregate["m"], 2);
    assert_eq!(aggregate["z"], 1);
}

#[test]
fn the_three_tables_are_written() {
    let temp = TempDir::new("report-tables");
    let mut result = built(1, "zlib");
    result.libs = vec!["m".to_string()];
    write_tables(temp.path(), "x86_64", &[result]).expect("tables");
    assert!(temp.join("x86_64/missing-symbols.tsv").is_file());
    assert!(temp.join("x86_64/headers.tsv").is_file());
    let libs = std::fs::read_to_string(temp.join("x86_64/libs.tsv")).expect("libs");
    assert!(libs.contains("m\t1\tstub"));
}

#[test]
fn repeated_writes_are_byte_identical() {
    let temp = TempDir::new("report-determinism");
    let results = vec![built(1, "zlib")];
    write_world(temp.path(), "x86_64", &results, &world()).expect("first");
    let first = std::fs::read(temp.join("x86_64/world.json")).expect("read");
    let md = std::fs::read(temp.join("x86_64/world.md")).expect("read");
    write_world(temp.path(), "x86_64", &results, &world()).expect("second");
    assert_eq!(
        first,
        std::fs::read(temp.join("x86_64/world.json")).expect("read")
    );
    assert_eq!(
        md,
        std::fs::read(temp.join("x86_64/world.md")).expect("read")
    );
}

#[test]
fn the_backlog_ranks_by_package_count_then_name() {
    let temp = TempDir::new("report-backlog");
    let mut first = BTreeMap::new();
    first.insert("beta".to_string(), 'U');
    first.insert("alpha".to_string(), 'U');
    let mut second = BTreeMap::new();
    second.insert("beta".to_string(), 'U');
    write_undef(temp.path(), "x86_64", "a", &first).expect("a");
    write_undef(temp.path(), "x86_64", "b", &second).expect("b");
    let index = SymbolIndex::from_names(&["beta"], &[]);
    let text = missing_backlog(temp.path(), "x86_64", &index, Path::new("/res")).expect("backlog");
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(lines[0], "symbol\tpackages\tstrong\tweak\tring");
    assert_eq!(lines[1], "beta\t2\t2\t0\t1");
    assert_eq!(lines[2], "alpha\t1\t1\t0\t-");
    assert!(text.contains("--resources /res"));
}

#[test]
fn the_stub_list_matches_c33() {
    assert!(is_stub_library("m"));
    assert!(is_stub_library("pthread"));
    assert!(is_stub_library("execinfo"));
    assert!(!is_stub_library("z"));
}

#[test]
fn a_placeholder_result_is_recognised() {
    assert!(is_placeholder(&PackageResult::new(
        1,
        "zlib",
        "PLACEHOLDER",
        "placeholder"
    )));
    assert!(!is_placeholder(&built(1, "zlib")));
}

#[test]
fn a_package_undef_file_can_be_read_by_name() {
    let temp = TempDir::new("report-package-undef");
    let mut map = BTreeMap::new();
    map.insert("strlcpy".to_string(), 'U');
    write_undef(temp.path(), "x86_64", "zlib", &map).expect("write");
    assert_eq!(
        package_undef(temp.path(), "x86_64", "zlib").expect("read"),
        map
    );
}
