// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

use super::*;
use crate::model::PackageResult;

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
optional = true
"#;

fn world() -> World {
    World::parse(WORLD).expect("world")
}

/// A world with `count` non-optional packages named `pkg1`..`pkgN`.
fn world_with(count: usize) -> World {
    let mut text = String::from("schema = 1\nfreeze = \"pending\"\nweights = { optional = 0.5 }\n");
    for n in 1..=count {
        text.push_str(&format!(
            "[[package]]\nn = {n}\nname = \"pkg{n}\"\ntier = 1\nbuild = \"mk\"\nversion = \"PLACEHOLDER\"\nurl = \"PLACEHOLDER\"\nsha256 = \"PLACEHOLDER\"\ntarball = \"PLACEHOLDER\"\nstresses = \"stdio\"\n"
        ));
    }
    World::parse(&text).expect("world")
}

fn built(n: u32, name: &str) -> PackageResult {
    PackageResult::new(n, name, "1.0", "built")
}

fn many(count: usize) -> Vec<PackageResult> {
    (1..=count)
        .map(|index| built(index as u32, &format!("pkg{index}")))
        .collect()
}

#[test]
fn results_outside_the_world_list_do_not_count_as_built() {
    let report = evaluate(&many(3), &world(), 1, 3);
    assert!(!report.met, "{:?}", report.lines);
    assert!(report.lines[0].contains("0 of 3"));
}

#[test]
fn a_hundred_of_a_hundred_meets_the_gate() {
    let report = evaluate(&many(100), &world_with(100), 1, 100);
    assert!(report.met, "{:?}", report.lines);
}

#[test]
fn ninety_nine_of_a_hundred_does_not_meet_the_gate() {
    let report = evaluate(&many(99), &world_with(100), 1, 100);
    assert!(!report.met);
    assert!(report.lines[0].contains("99 of 100"));
    assert!(report.lines[0].contains("NOT MET"));
}

#[test]
fn the_minimum_three_fixture_case_meets_the_gate() {
    let report = evaluate(&many(3), &world_with(3), 1, 3);
    assert!(report.met, "{:?}", report.lines);
}

#[test]
fn one_libc_patch_refuses_the_gate() {
    let mut results = many(3);
    results[1].libc_patches = 1;
    let report = evaluate(&results, &world_with(3), 1, 3);
    assert!(!report.met);
    assert!(report.lines[0].contains("libc_patches 1"));
    assert!(report.lines[0].contains("NOT MET"));
    assert!(
        report
            .lines
            .iter()
            .any(|line| line.contains("package `pkg2`"))
    );
}

#[test]
fn a_libc_disabled_configure_flag_refuses_the_gate() {
    let mut results = many(3);
    results[0]
        .configure_flags_disabled_because_libc
        .push("--disable-nls".to_string());
    let report = evaluate(&results, &world_with(3), 1, 3);
    assert!(!report.met);
    assert!(
        report
            .lines
            .iter()
            .any(|line| line.contains("package `pkg1`"))
    );
}

#[test]
fn a_libc_probe_failure_in_a_built_package_refuses_the_gate() {
    let mut results = many(3);
    results[2].libc_probe_failed.push("strlcpy".to_string());
    let report = evaluate(&results, &world_with(3), 1, 3);
    assert!(!report.met);
    assert!(report.lines.iter().any(|line| line.contains("`strlcpy`")));
}

#[test]
fn a_probe_failure_in_an_unbuilt_package_is_not_counted() {
    let mut results = many(3);
    results[2].status = "failed".to_string();
    results[2].libc_probe_failed.push("strlcpy".to_string());
    let report = evaluate(&results, &world_with(3), 1, 2);
    assert!(report.met, "{:?}", report.lines);
}

#[test]
fn zero_built_packages_are_not_met_even_with_zero_minimum() {
    let report = evaluate(&[], &world(), 1, 0);
    assert!(!report.met);
    assert!(
        report
            .lines
            .iter()
            .any(|line| line.contains("no package results"))
    );
}

#[test]
fn phase_two_requires_every_non_optional_package() {
    let mut results = many(2);
    results[1].status = "failed".to_string();
    let report = evaluate(&results, &world_with(2), 2, 0);
    assert!(!report.met);
    assert!(report.lines[0].contains("phase 2"));

    let results = many(2);
    let report = evaluate(&results, &world_with(2), 2, 0);
    assert!(report.met);
}

#[test]
fn phase_two_counts_every_non_optional_world_package() {
    let report = evaluate(&many(1), &world_with(2), 2, 0);
    assert!(!report.met, "{:?}", report.lines);
    assert!(report.lines[0].contains("1 of 2"));
}

#[test]
fn optional_rows_do_not_count_towards_the_phase_two_total() {
    let results = vec![built(1, "zlib")];
    let report = evaluate(&results, &world(), 2, 0);
    assert!(report.met, "{:?}", report.lines);
    assert!(report.lines[0].contains("1 of 1"));
}

#[test]
fn the_gate_line_has_the_c13_shape() {
    let report = evaluate(&many(1), &world_with(1), 1, 1);
    assert_eq!(
        report.lines[0],
        "C13 world build: 1 of 1 non-optional packages built; libc_patches 0 -> MET"
    );
}

#[test]
fn a_libc_patch_line_is_always_reported() {
    let report = evaluate(&many(1), &world_with(1), 1, 1);
    assert!(
        report
            .lines
            .iter()
            .any(|line| line == "C13 libc patches: 0 -> MET")
    );
}
