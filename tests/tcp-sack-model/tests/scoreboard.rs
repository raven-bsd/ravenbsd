// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Adapter and randomized checks against the merged K268 `SackScoreboard`.

use raven_tcpip::SackScoreboard;
use tcp_sack_model::{
    Rng, SackedSet, board_from_reference, classify_dsack, insert_absolute, relative_offset,
};

const SEED: u64 = 0x3070_5a43_4b53_4143; // K307 SACK model

fn compare(step: usize, board: &SackScoreboard, reference: &SackedSet) {
    assert_eq!(
        board.sacked,
        reference.runs(),
        "step {step}: merged ranges diverge"
    );
    assert_eq!(board.gaps(), reference.gaps(), "step {step}: gaps diverge");
    assert!(
        reference.invariants_hold(),
        "step {step}: reference invariants"
    );
}

#[test]
fn overlapping_blocks_match_the_byte_set_reference() {
    let mut board = SackScoreboard::new(100, 0, 0, 10, 3, 64);
    let mut reference = SackedSet::new(100);
    for (start, end) in [(10, 20), (15, 30), (30, 40), (5, 12)] {
        board.insert_block(start, end);
        reference.insert(start, end);
        compare(0, &board, &reference);
    }
}

#[test]
fn ten_thousand_random_insert_and_remove_operations_match_the_reference() {
    let high_data = 512;
    let mss = 64;
    let threshold = 3;
    let mut board = SackScoreboard::new(high_data, 0, 0, mss, threshold, 64);
    let mut reference = SackedSet::new(high_data);
    let mut rng = Rng::new(SEED);
    let mut removes = 0usize;
    let mut inserts = 0usize;

    for step in 0..10_000 {
        if rng.below(10) < 8 {
            let start = i64::from(rng.range(0, 600)) - 32;
            let end = start + i64::from(rng.range(1, 120));
            board.insert_block(start, end);
            reference.insert(start, end);
            inserts += 1;
        } else if let Some((_, first_end)) = reference.runs().first().copied() {
            // Remove a complete first block. This avoids the separate reneging
            // path, which is covered by its own test below.
            board.ack_blocks(first_end);
            reference.ack(first_end);
            removes += 1;
        }
        compare(step, &board, &reference);
        if step % 100 == 0 {
            assert_eq!(
                board.pipe(),
                reference.pipe(board.high_rxt, threshold, mss),
                "step {step}: pipe diverges"
            );
            assert_eq!(
                board.lost_ranges(),
                reference.lost_ranges(threshold, mss),
                "step {step}: lost ranges diverge"
            );
        }
    }
    assert!(inserts > 7_000, "the randomized run must insert");
    assert!(removes > 100, "the randomized run must remove");
    println!("tcp-sack-model: seed={SEED:#x} inserts={inserts} removes={removes}");
}

#[test]
fn the_range_cap_is_bounded_and_reports_drops() {
    let mut board = SackScoreboard::new(1_000, 0, 0, 100, 3, 3);
    let mut reference = SackedSet::new(1_000);
    for index in 0..5u32 {
        let start = i64::from(index) * 100;
        board.insert_block(start, start + 50);
        reference.insert(start, start + 50);
    }
    assert_eq!(board.sacked.len(), 3);
    assert!(board.dropped >= 2);
    for (start, end) in &board.sacked {
        for byte in *start..*end {
            assert!(reference.bytes.contains(&byte), "retained byte {byte}");
        }
    }
}

#[test]
fn a_full_scoreboard_drops_an_overlapping_block_before_merging() {
    // K268 behavior: `insert_block` checks the cap before `normalize`, so a
    // block that would merge into the first range is dropped at the cap.
    let mut board = SackScoreboard::new(1_000, 0, 0, 100, 3, 1);
    board.insert_block(100, 200);
    board.insert_block(150, 250);
    assert_eq!(board.sacked, vec![(100, 200)]);
    assert_eq!(board.dropped, 1);
}

#[test]
fn dsack_blocks_are_classified_and_skipped() {
    assert_eq!(classify_dsack(&[(-100, 10)], 0), 1);
    assert_eq!(classify_dsack(&[(100, 200), (50, 250)], 0), 2);
    let mut board = SackScoreboard::new(1_000, 0, 0, 100, 3, 64);
    board.add_blocks(0, 1_000, &[(0, 100), (200, 300)]);
    assert_eq!(board.sacked, vec![(200, 300)]);
    board.note_dsack(0, 1);
    let result = board.next_seg(false, 0);
    assert!(result.dsack_detected);
}

#[test]
fn reneging_clears_the_scoreboard() {
    let mut board = SackScoreboard::new(1_000, 0, 0, 100, 3, 64);
    board.insert_block(100, 200);
    board.ack_blocks(150);
    assert!(board.is_reneging());
    assert!(board.sacked.is_empty());
}

#[test]
fn duplicate_sack_blocks_are_idempotent() {
    let mut board = SackScoreboard::new(1_000, 0, 0, 100, 3, 64);
    board.insert_block(100, 200);
    board.insert_block(100, 200);
    board.add_blocks(0, 1_000, &[(100, 200), (100, 200)]);
    assert_eq!(board.sacked, vec![(100, 200)]);
}

#[test]
fn sequence_wrap_maps_to_relative_offsets() {
    let snd_una = 0xFFFF_FFF0;
    assert_eq!(relative_offset(0xFFFF_FFF8, snd_una), 8);
    assert_eq!(relative_offset(0x0000_0010, snd_una), 32);
    let mut board = SackScoreboard::new(0x40, 0, 0, 64, 3, 16);
    let mut reference = SackedSet::new(0x40);
    insert_absolute(&mut board, 0xFFFF_FFF8, 0x0000_0010, snd_una);
    reference.insert(8, 32);
    assert_eq!(board.sacked, reference.runs());
    insert_absolute(&mut board, 0xFFFF_FFFC, 0x0000_0008, snd_una);
    reference.insert(12, 24);
    assert_eq!(board.sacked, reference.runs());
}

#[test]
fn receiver_block_order_is_dsack_first_then_newest_with_the_option_cap() {
    let holes = [(10, 20), (30, 40), (50, 60), (70, 80), (90, 100)];
    let with_timestamps = raven_tcpip::receiver_blocks(&holes, true, Some((0, 5)));
    assert_eq!(with_timestamps, vec![(0, 5), (90, 100), (70, 80)]);
    let without_timestamps = raven_tcpip::receiver_blocks(&holes, false, None);
    assert_eq!(
        without_timestamps,
        vec![(90, 100), (70, 80), (50, 60), (30, 40)]
    );
}

#[test]
fn k268_sacked_bytes_above_counts_a_block_byte_at_the_point() {
    // K268 finding: `SackScoreboard::sacked_bytes_above` includes the byte
    // at `point`, while RFC 6675 "above SeqNum" is strict. The reference and
    // the product therefore differ by one at a block start. This is reported
    // to K268; the model does not patch product source.
    let mut board = SackScoreboard::new(100, 0, 0, 10, 3, 64);
    board.insert_block(50, 70);
    assert_eq!(board.sacked_bytes_above(50), 20);
    let mut reference = SackedSet::new(100);
    reference.insert(50, 70);
    assert_eq!(reference.sacked_bytes_above(50), 19);
}

#[test]
fn a_board_built_from_the_reference_has_the_same_pipe_and_lost_bytes() {
    let mut reference = SackedSet::new(400);
    reference.insert(100, 160);
    reference.insert(200, 260);
    let board = board_from_reference(&reference, 220, 0, 64, 3, 64);
    assert_eq!(board.sacked, reference.runs());
    assert_eq!(board.pipe(), reference.pipe(220, 3, 64));
    assert_eq!(board.lost_ranges(), reference.lost_ranges(3, 64));
}
