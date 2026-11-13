// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! T4: Rust == C constants. Every spec row, the family-prefix scan and the
//! errno parity against the C36 source table.

use std::collections::BTreeMap;

use raven_libc_hdrcheck::clang::{self, Arch, Lang, Tu, Vis};
use raven_libc_hdrcheck::{consts, manifest, matrix};

#[test]
fn every_spec_constant_asserts() {
    let toolchain = clang::locate_toolchain().expect("clang");
    let rows = consts::load_spec().expect("spec-constants.toml");
    assert!(rows.len() >= 60, "only {} spec constants", rows.len());
    let mut failures = 0;
    for arch in Arch::ALL {
        let source = consts::assert_source(&rows, arch);
        let outcome = clang::compile(
            &toolchain,
            &Tu {
                arch,
                lang: Lang::Gnu17,
                vis: Vis::Default,
                source: source.clone(),
            },
        );
        if !outcome.ok {
            failures += 1;
            eprintln!(
                "FAIL spec constants on {arch}:\n{}\n--- source ---\n{source}",
                outcome.stderr
            );
        }
    }
    println!(
        "T4(a): {} constants x {} arches asserted",
        rows.len(),
        Arch::ALL.len()
    );
    assert_eq!(failures, 0, "T4(a) failures: {failures}");
}

#[test]
fn family_prefix_scan_matches_the_generator() {
    let toolchain = clang::locate_toolchain().expect("clang");
    let rows = manifest::load().expect("manifest");
    let headers = manifest::owned_standalone(&rows);
    let spec = consts::load_spec().expect("spec");
    let header_only_rows = consts::load_header_only().expect("header-only");
    let required = manifest::required_owners();
    let complete = manifest::require_complete();
    let mut header_only = BTreeMap::new();
    for row in &header_only_rows {
        header_only.insert(row.name.clone(), row.reason.clone());
    }
    // Names from spec-constants.toml are legitimate too; they are checked by
    // T4(a), so do not report them as unlisted.
    for row in &spec {
        header_only.entry(row.name.clone()).or_insert_with(|| {
            "asserted by spec-constants.toml; value checked in T4(a)".to_string()
        });
    }
    let generated = consts::generated_defines().expect("generated defines");

    let mut source = String::new();
    for header in &headers {
        source.push_str(&format!("#include <{header}>\n"));
    }
    source.push_str(matrix::MAIN_TAIL);
    let mut failures = 0;
    for arch in Arch::ALL {
        let dump = clang::macro_dump(&toolchain, arch, Lang::Gnu17, &source, &[])
            .unwrap_or_else(|error| panic!("cannot dump macros on {arch}: {error}"));
        let (mismatches, unlisted) = consts::scan_families(&dump, &generated, &header_only);
        for (name, actual, expected) in &mismatches {
            // A name owned by a pending ticket is PENDING, never a pass.
            if let Some(row) = header_only_rows.iter().find(|row| &row.name == name)
                && row.owner != "C37"
            {
                if complete || required.iter().any(|owner| owner == &row.owner) {
                    failures += 1;
                    eprintln!("PENDING constant {arch} {name} (owner {})", row.owner);
                } else {
                    println!("PENDING {arch} {name} (owner {})", row.owner);
                }
                continue;
            }
            failures += 1;
            eprintln!("MISMATCH {arch} {name}: header {actual} != generated {expected}");
        }
        for name in unlisted {
            failures += 1;
            eprintln!("UNLISTED {arch} {name}");
        }
        println!(
            "T4(b): {arch}: {} family defines checked, {} mismatches",
            dump.iter()
                .filter(|(name, _)| consts::in_family(name))
                .count(),
            mismatches.len()
        );
    }
    assert_eq!(failures, 0, "T4(b) failures: {failures}");
}

#[test]
fn errno_parity_with_c36() {
    let (numbers, aliases) = consts::errno_parity().expect("errno parity");
    assert_eq!(numbers, consts::linux_errno_count() + 14);
    assert_eq!(consts::linux_errno_count(), 131);
    assert_eq!(aliases, 4);
    println!("T4(c): {numbers} errno numbers, {aliases} aliases, 41/58 absent");
}
