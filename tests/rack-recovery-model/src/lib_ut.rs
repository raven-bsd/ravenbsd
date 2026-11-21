// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Golden trace, recovery/pipe vectors and negative checks.

use super::*;
use raven_tcpip::Rack;

fn golden_path() -> String {
    format!(
        "{}/traces/rack-recovery-10k.tsv",
        env!("CARGO_MANIFEST_DIR")
    )
}

#[test]
fn the_10k_golden_trace_regenerates_exactly() {
    let expected = format!("{}{}\n", header(), generate(TRACE_COUNT).join("\n"));
    let actual = std::fs::read_to_string(golden_path()).unwrap_or_else(|error| {
        panic!(
            "missing golden trace ({error}); regenerate with: cargo run -q -p rack-recovery-model -- \
             --generate tests/rack-recovery-model/traces/rack-recovery-10k.tsv"
        )
    });
    assert_eq!(actual, expected, "the golden trace is stale");

    let rows: Vec<&str> = actual
        .lines()
        .filter(|line| !line.starts_with('#'))
        .collect();
    assert_eq!(rows.len(), usize::try_from(TRACE_COUNT).unwrap_or(0));
    for row in rows {
        let fields: Vec<&str> = row.split('\t').collect();
        assert_eq!(fields.len(), 7, "row columns: {row}");
        let pipe: u32 = fields[3].parse().expect("pipe");
        assert!(pipe <= SEGMENTS as u32 * MSS, "pipe {pipe} > sent stream");
        let (entries, exits) = fields[5].split_once('/').expect("entries/exits");
        let entries: u32 = entries.parse().expect("entries");
        let exits: u32 = exits.parse().expect("exits");
        assert!(exits <= entries, "exits {exits} > entries {entries}");
        if fields[2] != "-" {
            let mut previous = 0u32;
            for range in fields[2].split(',') {
                let (start, end) = range.split_once('-').expect("lost range");
                let start: u32 = start.parse().expect("lost start");
                let end: u32 = end.parse().expect("lost end");
                assert!(start < end, "empty lost range");
                assert!(start >= previous, "lost ranges out of order");
                previous = start;
                assert!(
                    start >= FIRST_SEQ && end <= FIRST_SEQ + SEGMENTS as u32 * MSS,
                    "lost range outside the send map"
                );
            }
        }
    }
}

#[test]
fn recovery_entry_exit_and_pipe_are_exact() {
    let mut model = RecoveryModel::new(Config::default());
    for index in 0..SEGMENTS {
        let start = FIRST_SEQ + index as u32 * MSS;
        model.send(start, start + MSS, index as u32 * 5);
    }
    model.on_sack(8_000, 9_000, 100);
    assert_eq!(model.recovery_entries, 1);
    assert_eq!(model.lost, vec![(1_000, 8_000)]);
    assert_eq!(model.select(100), Some((1_000, 2_000)));
    assert_eq!(
        model.pipe(),
        1_000,
        "pipe includes the retransmitted segment"
    );
    let outcome = model.outcome();
    assert_eq!(outcome.retransmits, vec![(1_000, 2_000)]);
    assert_eq!(outcome.recovery_exits, 0);

    model.on_ack(9_000, 200);
    let outcome = model.outcome();
    assert_eq!(outcome.recovery_entries, 1);
    assert_eq!(outcome.recovery_exits, 1);
    assert!(outcome.lost.is_empty());
    assert_eq!(outcome.pipe, 0);
}

#[test]
fn the_shared_core_matches_the_product_loss_bound() {
    let mut product = Rack::new(0);
    product.on_sent(2_000, 0);
    let mut model = RecoveryModel::new(Config::default());
    model.send(1_000, 2_000, 0);
    assert_eq!(product.is_lost(100, 100), model.inner.is_lost(1_000, 100));
    assert_eq!(product.is_lost(20, 100), model.inner.is_lost(1_000, 20));
}

#[test]
fn a_corrupted_golden_row_is_detected() {
    let rows = generate(10);
    let mut corrupted = rows.clone();
    let mut fields: Vec<String> = corrupted[3].split('\t').map(str::to_string).collect();
    let pipe: u32 = fields[3].parse().expect("pipe");
    fields[3] = (pipe + 1).to_string();
    corrupted[3] = fields.join("\t");
    assert_ne!(
        corrupted, rows,
        "the trace comparison must catch a changed pipe"
    );
}
