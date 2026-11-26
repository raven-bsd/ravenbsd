// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! SACK/DSACK matrix, K319/K324 trace replay and K315 group checks.

use std::fs;

use raven_tcpip::{RACK_ACKED, RACK_SACK_PASSED, RACK_WAS_LOST, RackSack};

use crate::reference::ReferenceSack;

fn product() -> RackSack {
    let mut sack = RackSack::new();
    sack.stack.tp.snd_una = 0;
    sack.stack.tp.snd_max = 100_000;
    sack.threshold_us = 100;
    for index in 0..100u32 {
        sack.insert_segment(index * 1_000, (index + 1) * 1_000, 0, 1);
    }
    sack
}

fn next(state: &mut u64) -> u64 {
    *state ^= *state << 13;
    *state ^= *state >> 7;
    *state ^= *state << 17;
    *state
}

fn assert_matches(product: &RackSack, reference: &ReferenceSack) {
    assert_eq!(product.sacked_ranges, reference.ranges, "sacked ranges");
    assert_eq!(
        product.sacked_bytes(),
        reference.sacked_bytes(),
        "sacked bytes"
    );
    assert_eq!(
        product.stack.sacked,
        reference.sacked_bytes(),
        "stack count"
    );
    assert!(product.stack.tp.snd_una <= product.stack.tp.snd_max);
    let mut last_end = 0u32;
    for (start, end) in &product.sacked_ranges {
        assert!(start < end, "empty/negative range");
        assert!(*start >= last_end, "ranges overlap or are unsorted");
        assert!(
            *start >= product.stack.tp.snd_una,
            "SACK range below cumack"
        );
        last_end = *end;
    }
}

#[test]
fn sack_block_split_merge_and_flags() {
    let mut sack = product();
    assert_eq!(sack.rack_proc_sack_blk(50, 150, 1_000, 1_460), 100);
    assert_eq!(sack.sacked_ranges, vec![(50, 150)]);
    assert_eq!(sack.sacked_bytes(), 100);
    assert_eq!(sack.stack.sacked, 100);
    assert_eq!(sack.sendmap[&0].end, 50);
    assert_eq!(sack.sendmap[&50].flags & RACK_ACKED, RACK_ACKED);
    assert_eq!(sack.sendmap[&150].flags & RACK_ACKED, 0);
    assert_eq!(sack.sendmap[&0].flags & RACK_WAS_LOST, RACK_WAS_LOST);
}

#[test]
fn sack_passed_threshold_and_stop() {
    let mut sack = product();
    sack.threshold_us = 100;
    for key in [0u32, 1_000, 2_000] {
        let segment = sack.sendmap.get_mut(&key).expect("segment");
        let slot = usize::from(segment.rtr_cnt.saturating_sub(1));
        segment.tim_lastsent[slot] = 0;
    }
    assert_eq!(sack.rack_log_sack_passed(3_000, 100), 3_000);
    assert_eq!(sack.stack.considered_lost, 3_000);
    for key in [0u32, 1_000, 2_000] {
        let segment = &sack.sendmap[&key];
        assert_eq!(segment.flags & RACK_WAS_LOST, RACK_WAS_LOST);
        assert_eq!(segment.flags & RACK_SACK_PASSED, RACK_SACK_PASSED);
    }
    assert_eq!(sack.rack_log_sack_passed(3_000, 200), 0);
}

#[test]
fn peer_reneges_and_cumack_trim_match_reference() {
    let mut sack = product();
    let mut reference = ReferenceSack::new();
    sack.rack_proc_sack_blk(1_000, 2_000, 1_000, 1_460);
    reference.sack_block(1_000, 2_000);
    sack.rack_proc_sack_blk(3_000, 4_000, 1_000, 1_460);
    reference.sack_block(3_000, 4_000);
    assert_matches(&sack, &reference);

    assert_eq!(sack.rack_peer_reneges(3_500), 1_500);
    reference.trim_below(3_500);
    assert_matches(&sack, &reference);
    assert_eq!(sack.sacked_ranges, vec![(3_500, 4_000)]);

    assert_eq!(sack.rack_process_to_cumack(3_750, 1_000, 1), 3_750);
    reference.trim_below(3_750);
    assert_matches(&sack, &reference);
    assert_eq!(sack.stack.tp.snd_una, 3_750);
    assert_eq!(sack.sacked_ranges, vec![(3_750, 4_000)]);
}

#[test]
fn dsack_tlp_round_and_spurious_rules() {
    let mut tlp = RackSack::new();
    tlp.last_tlp_acked = Some((100, 200));
    assert!(tlp.rack_note_dsack(120, 180));
    assert_eq!(tlp.dsack_byte_cnt, 60);
    assert_eq!(tlp.stack.num_dsacks, 0);
    assert!(!tlp.dsack_round_seen);

    let mut sack = RackSack::new();
    sack.stack.tp.snd_max = 1_000;
    assert!(!sack.rack_note_dsack(0, 100));
    assert!(sack.dsack_round_seen);
    assert_eq!(sack.dsack_round_end, 1_000);
    assert_eq!(sack.stack.num_dsacks, 1);
    assert_eq!(sack.dsack_persist, 16);
    assert_eq!(sack.dsack_byte_cnt, 100);

    sack.retran_during_recovery = 50;
    assert!(!sack.rack_note_dsack(100, 200));
    assert!(sack.might_revert);
    assert!(sack.spurious_recovery);
    assert_eq!(sack.dsack_byte_cnt, 0);
    assert_eq!(sack.retran_during_recovery, 0);
}

#[test]
fn sack_filter_and_k314_log_event() {
    let mut sack = product();
    assert_eq!(
        sack.sack_filter_blks(&[(0, 100), (100, 200), (300, 400)], 50),
        2
    );
    assert_eq!(sack.sack_filter, vec![(50, 200), (300, 400)]);
    sack.sack_filter_clear(150);
    assert!(sack.sack_filter.is_empty());
    sack.rack_log_dsack_event(5, 1, 2, 3);
    assert_eq!(sack.last_event, Some(64));
    assert_eq!(sack.last_dsack_log, Some([5, 1, 2, 3]));
}

#[test]
fn k336_flag_bits_match_public_rack_constants() {
    let mut sack = product();
    sack.threshold_us = 0;
    assert_eq!(sack.rack_log_sack_passed(1_000, 0), 1_000);
    let flags = sack.sendmap[&0].flags;
    assert_eq!(flags & RACK_WAS_LOST, RACK_WAS_LOST);
    assert_eq!(RACK_ACKED, 0x0000_0001);
    assert_eq!(RACK_SACK_PASSED, 0x0000_0010);
    assert_eq!(RACK_WAS_LOST, 0x0040_0000);
}

#[test]
fn k315_group_rows_are_present_exactly_once() {
    let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../rack-coverage/fixtures/rack-functions.txt");
    let text = fs::read_to_string(&path).unwrap_or_else(|error| panic!("{path:?}: {error}"));
    let functions = [
        "rack_log_dsack_event",
        "rack_log_sack_passed",
        "rack_proc_sack_blk",
        "rack_peer_reneges",
        "rack_process_to_cumack",
        "rack_note_dsack",
    ];
    for function in functions {
        let count = text
            .lines()
            .filter(|line| line.split_whitespace().nth(1) == Some(function))
            .count();
        assert_eq!(count, 1, "K315 donor row {function} must appear once");
    }
}

fn run_seed(seed: u64) {
    let mut sack = product();
    let mut reference = ReferenceSack::new();
    let mut state = seed | 1;
    for _ in 0..12 {
        let random = next(&mut state);
        let snd_una = sack.stack.tp.snd_una;
        let room = 100_000u32.saturating_sub(snd_una);
        if room < 2 {
            break;
        }
        match random % 10 {
            0..=5 => {
                let start = snd_una + u32::try_from(random % u64::from(room - 1)).unwrap_or(0);
                let length = 1 + u32::try_from((random >> 20) % 4_096).unwrap_or(0);
                let end = start.saturating_add(length).min(100_000).max(start + 1);
                if end > start {
                    let before = sack.sacked_bytes();
                    let product_changed = sack.rack_proc_sack_blk(start, end, 1_000, 1_460);
                    let reference_changed = reference.sack_block(start, end);
                    assert_eq!(
                        product_changed, reference_changed,
                        "seed {seed} SACK {start}-{end} changed (before {before})"
                    );
                }
            }
            6..=7 => {
                let ack = snd_una + u32::try_from(random % u64::from(room)).unwrap_or(0);
                let _ = sack.rack_process_to_cumack(ack, 1_000, random);
                let _ = reference.trim_below(ack);
                // The product returns freed sendmap bytes, while this reference tracks only
                // SACKed ranges; compare the observable state, not that different counter.
                assert_eq!(sack.stack.tp.snd_una, ack);
            }
            8 => {
                let ack = snd_una + u32::try_from(random % u64::from(room)).unwrap_or(0);
                let product_removed = sack.rack_peer_reneges(ack);
                let reference_removed = reference.trim_below(ack);
                assert_eq!(
                    product_removed, reference_removed,
                    "seed {seed} renege {ack}"
                );
            }
            _ => {
                let start = snd_una + u32::try_from(random % u64::from(room)).unwrap_or(0);
                let length = 1 + u32::try_from((random >> 16) % 2_048).unwrap_or(0);
                let end = start.saturating_add(length).min(100_000).max(start + 1);
                let product_tlp = sack.rack_note_dsack(start, end);
                assert!(!product_tlp, "no TLP marker in the random matrix");
                reference.note_dsack(start, end);
                assert_eq!(
                    sack.stack.num_dsacks, reference.num_dsacks,
                    "seed {seed} DSACK rounds"
                );
                assert_eq!(
                    sack.dsack_byte_cnt, reference.dsack_byte_cnt,
                    "seed {seed} DSACK bytes"
                );
                assert_eq!(sack.dsack_persist, reference.dsack_persist);
            }
        }
        assert_matches(&sack, &reference);
    }
}

fn trace_seeds(path: &std::path::Path) -> Vec<u64> {
    let text = fs::read_to_string(path).unwrap_or_else(|error| panic!("{path:?}: {error}"));
    text.lines()
        .filter(|line| !line.starts_with('#') && !line.trim().is_empty())
        .filter_map(|line| line.split_whitespace().next()?.parse().ok())
        .collect()
}

#[test]
fn k319_and_k324_trace_seeds_replay_as_one_matrix() {
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let k319 = trace_seeds(&root.join("tests/rack-loss-model/traces/rack-10k.tsv"));
    let k324 = trace_seeds(&root.join("tests/rack-recovery-model/traces/rack-recovery-10k.tsv"));
    assert_eq!(k319.len(), 10_000);
    assert_eq!(k324.len(), 10_000);
    for seed in k319.iter().chain(k324.iter()) {
        run_seed(*seed);
    }
    assert_eq!(k319.len() + k324.len(), 20_000);
}
