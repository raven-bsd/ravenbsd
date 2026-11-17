// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Deterministic UFS crash-cut matrix.

use std::collections::BTreeMap;

use raven_fs_fuzz_replay::{crash_matrix, journal_crash_matrix};

#[test]
fn every_write_cut_recovers_or_fails_closed() {
    let report = crash_matrix().expect("crash matrix");
    assert!(report.total_writes > 0, "the probe sequence wrote nothing");
    assert_eq!(
        report.cuts.len() as u64,
        report.total_writes + 1,
        "one row per write budget"
    );
    let mut by_stage: BTreeMap<&str, (u64, u64, u64, u64)> = BTreeMap::new();
    let mut clean = 0u64;
    let mut repaired = 0u64;
    let mut refused = 0u64;
    for cut in &report.cuts {
        let entry = by_stage.entry(cut.stage.as_str()).or_default();
        entry.0 += 1;
        if cut.check == "clean" {
            clean += 1;
            entry.1 += 1;
        } else if cut.check == "repaired" {
            repaired += 1;
            entry.1 += 1;
        } else {
            assert!(
                cut.check.starts_with("fail-closed")
                    || cut.check.starts_with("repair-failed")
                    || cut.check.starts_with("repair-incomplete"),
                "cut {} has unknown check result {}",
                cut.cut,
                cut.check
            );
            refused += 1;
            entry.3 += 1;
        }
    }
    for (stage, counts) in &by_stage {
        println!(
            "crash stage {stage}: cuts={} recovered={} refused={}",
            counts.0, counts.1, counts.3
        );
    }
    println!(
        "crash total: writes={} cuts={} clean={} repaired={} refused={}",
        report.total_writes,
        report.cuts.len(),
        clean,
        repaired,
        refused
    );
    for cut in &report.cuts {
        if cut.check != "clean" && cut.check != "repaired" {
            println!(
                "crash refused cut={} stage={} operation={} mount={} check={}",
                cut.cut, cut.stage, cut.operation, cut.mount, cut.check
            );
        }
    }
    assert!(clean + repaired > 0, "no cut recovered");
    assert!(
        refused > 0,
        "fault injection did not produce a detected or failed-closed cut"
    );
}

#[test]
fn every_journaled_write_cut_replays_or_refuses() {
    let report = journal_crash_matrix().expect("journal crash matrix");
    assert!(report.total_writes > 0, "the journaled probe wrote nothing");
    assert_eq!(
        report.cuts.len() as u64,
        report.total_writes + 1,
        "one row per write budget"
    );
    for cut in &report.cuts {
        assert!(
            cut.replay == "replayed" || cut.replay.starts_with("refused"),
            "cut {} failed: {} ({} / {})",
            cut.cut,
            cut.replay,
            cut.stage,
            cut.operation
        );
    }
    println!(
        "journal crash total: writes={} cuts={} replayed={} refused={} failed={}",
        report.total_writes,
        report.cuts.len(),
        report.replayed,
        report.refused,
        report.failed
    );
    assert!(report.replayed > 0, "no journaled cut replayed");
    assert_eq!(report.failed, 0, "a journaled cut failed to recover");
}
