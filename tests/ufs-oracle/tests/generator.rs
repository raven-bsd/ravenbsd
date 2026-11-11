// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Runs the fixed seed set (20 seeds x 5,000 operations by default).

use ufs_oracle_tests::{Config, run};

#[test]
fn differential_seeds_are_clean() {
    let config = Config::from_env().expect("oracle configuration");
    let summary = run(&config).expect("differential run");
    assert_eq!(summary.seeds.len() as u64, config.seeds);
    for report in &summary.seeds {
        println!("{}", report.line());
        assert!(report.ok(), "{}", report.line());
    }
    println!(
        "summary: seeds={} operations={} checks={}",
        summary.seeds.len(),
        summary
            .seeds
            .iter()
            .map(|report| report.operations)
            .sum::<u32>(),
        summary
            .seeds
            .iter()
            .map(|report| report.checks)
            .sum::<u32>()
    );
}
