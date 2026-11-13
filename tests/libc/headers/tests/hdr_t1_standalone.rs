// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! T1: compile every owned standalone header for every arch, language and
//! visibility mode (doc 14 3.B.4).

use raven_libc_hdrcheck::clang::{self, Arch, Lang, Tu, Vis};
use raven_libc_hdrcheck::{manifest, matrix};

#[test]
fn every_owned_header_compiles_standalone() {
    let toolchain =
        clang::locate_toolchain().expect("clang must be installed (RAVEN_CLANG or PATH)");
    let rows = manifest::load().expect("manifest.toml");
    let headers = manifest::owned_standalone(&rows);
    assert!(!headers.is_empty(), "no owned standalone headers");
    let quick = manifest::quick();
    let tus = matrix::t1_tus(&headers, quick);
    let (arches, langs, visibilities) = if quick {
        (1, Lang::QUICK.len(), Vis::QUICK.len())
    } else {
        (Arch::ALL.len(), Lang::ALL.len(), Vis::ALL.len())
    };
    let expected = headers.len() * arches * langs * visibilities;
    println!(
        "T1: {} headers x {} arches x {} languages x {} visibility modes = {} TUs",
        headers.len(),
        arches,
        langs,
        visibilities,
        tus.len()
    );
    assert_eq!(tus.len(), expected, "T1 TU count is not the formula");
    let results = clang::run_matrix(&toolchain, tus, manifest::jobs());
    let mut failures = 0;
    for (tu, outcome) in &results {
        if !outcome.ok {
            failures += 1;
            if failures <= 20 {
                eprintln!(
                    "FAIL {} {} {}\nargs: {:?}\n{}",
                    tu.arch,
                    tu.lang.name(),
                    tu.vis.name(),
                    clang::clang_args(&toolchain, tu),
                    outcome.stderr
                );
            }
        }
    }
    println!(
        "T1: {} passed, {} failed",
        results.len() - failures,
        failures
    );
    assert_eq!(failures, 0, "T1 failures: {failures}");
}

#[test]
fn t1_result_order_is_deterministic() {
    let toolchain = clang::locate_toolchain().expect("clang");
    let rows = manifest::load().expect("manifest");
    let headers: Vec<String> = manifest::owned_standalone(&rows)
        .into_iter()
        .take(2)
        .collect();
    let tus = matrix::t1_tus(&headers, false);
    let first = clang::run_matrix(&toolchain, tus.clone(), manifest::jobs());
    let second = clang::run_matrix(&toolchain, tus, manifest::jobs());
    let first: Vec<(Tu, bool)> = first
        .into_iter()
        .map(|(tu, outcome)| (tu, outcome.ok))
        .collect();
    let second: Vec<(Tu, bool)> = second
        .into_iter()
        .map(|(tu, outcome)| (tu, outcome.ok))
        .collect();
    assert_eq!(first, second);
}
