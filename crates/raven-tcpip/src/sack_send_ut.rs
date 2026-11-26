// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

use super::*;
use crate::Instant;
use crate::sack::{SackReport, ndsack_blocks, parse_sack_option};
use alloc::vec;

fn connection() -> TcpConnection {
    let mut connection = TcpConnection::new_established_for_test(Instant::ZERO);
    connection.snd_una = 1_000;
    connection.snd_nxt = 11_000;
    connection.mss = 1_000;
    connection.sack_permitted = true;
    connection
}

#[test]
fn blocks_merge_and_report_new_coverage() {
    let mut c = connection();
    assert!(merge_sack_blocks(&mut c, &[(3_000, 4_000), (3_500, 5_000)]));
    assert_eq!(c.sacked, vec![(3_000, 5_000)]);
    assert!(!merge_sack_blocks(&mut c, &[(3_000, 4_000)]));
    assert!(merge_sack_blocks(&mut c, &[(5_000, 6_000)]));
    assert_eq!(c.sacked, vec![(3_000, 6_000)]);
}

#[test]
fn dsack_and_future_blocks_are_ignored() {
    let mut c = connection();
    assert!(!merge_sack_blocks(&mut c, &[(500, 900)]));
    assert!(!merge_sack_blocks(&mut c, &[(20_000, 21_000)]));
    assert!(c.sacked.is_empty());
    assert!(merge_sack_blocks(&mut c, &[(10_000, 20_000)]));
    assert_eq!(c.sacked, vec![(10_000, 11_000)]);
}

#[test]
fn prune_drops_and_trims_acknowledged_ranges() {
    let mut c = connection();
    merge_sack_blocks(&mut c, &[(2_000, 3_000), (5_000, 7_000)]);
    prune_sacked(&mut c, 2_500);
    assert_eq!(c.sacked, vec![(2_500, 3_000), (5_000, 7_000)]);
    prune_sacked(&mut c, 3_000);
    assert_eq!(c.sacked, vec![(5_000, 7_000)]);
}

#[test]
fn scoreboard_uses_offsets_from_snd_una() {
    let mut c = connection();
    merge_sack_blocks(&mut c, &[(3_000, 6_000)]);
    let board = scoreboard(&c);
    assert_eq!(board.sacked, vec![(2_000, 5_000)]);
    assert_eq!(board.high_data, 10_000);
}

#[test]
fn dragonfly_lostseq_has_sacked_and_bytes_below() {
    let mut board = SackScoreboard::new(10_000, 0, 0, 1_000, 3, 16);
    board.insert_block(2_000, 3_000);
    board.insert_block(4_000, 6_000);
    assert_eq!(board.lostseq, 4_000);
    assert!(board.has_sacked(3_000));
    assert!(!board.has_sacked(3_001));
    assert_eq!(board.bytes_below(5_000), 2_000);
    assert_eq!(board.bytes_below(10_000), 3_000);
}

#[test]
fn head_is_lost_needs_three_segments_of_sacked_data() {
    let mut c = connection();
    merge_sack_blocks(&mut c, &[(2_000, 3_000)]);
    assert!(!head_is_lost(&c));
    merge_sack_blocks(&mut c, &[(2_000, 4_500)]);
    assert!(head_is_lost(&c));
    c.sack_permitted = false;
    assert!(!head_is_lost(&c));
}

#[test]
fn rto_recovery_marks_every_hole_below_a_sack_lost() {
    let mut c = connection();
    merge_sack_blocks(&mut c, &[(2_000, 2_500)]);
    assert!(!head_is_lost(&c));
    c.rto_recovery = true;
    assert!(head_is_lost(&c));
}

#[test]
fn pipe_excludes_sacked_and_lost_bytes() {
    let mut c = connection();
    merge_sack_blocks(&mut c, &[(2_000, 5_000)]);
    assert_eq!(sack_pipe(&c), 6_000);
    c.high_rxt = 2_000;
    assert_eq!(sack_pipe(&c), 7_000);
}

#[test]
fn dragonfly_ack_blocks_trims_and_detects_reneging() {
    let mut board = SackScoreboard::new(10_000, 0, 0, 1_000, 3, 16);
    board.insert_block(500, 800);
    board.insert_block(1_000, 3_000);
    board.ack_blocks(900);
    assert_eq!(board.sacked, vec![(1_000, 3_000)]);
    assert!(!board.is_reneging());
    board.ack_blocks(1_000);
    assert!(board.is_reneging());
    assert!(board.sacked.is_empty());
}

#[test]
fn dragonfly_add_blocks_skips_dsack_and_out_of_range() {
    let mut board = SackScoreboard::new(10_000, 0, 0, 1_000, 3, 16);
    board.add_blocks(0, 10_000, &[(-100, 0), (2_000, 3_000), (5_000, 6_000)]);
    assert_eq!(board.sacked, vec![(2_000, 3_000), (5_000, 6_000)]);
    board.add_blocks(0, 10_000, &[(3_000, 4_000), (11_000, 12_000)]);
    assert_eq!(board.sacked, vec![(2_000, 4_000), (5_000, 6_000)]);
}

#[test]
fn dragonfly_skip_sacked_and_first_unsacked_len() {
    let mut board = SackScoreboard::new(10_000, 0, 0, 1_000, 3, 16);
    board.insert_block(1_000, 3_000);
    assert_eq!(board.skip_sacked(1_500), 3_000);
    assert_eq!(board.skip_sacked(3_000), 3_000);
    assert_eq!(board.first_unsacked_len(), 1_000);
    let empty = SackScoreboard::new(10_000, 0, 0, 1_000, 3, 16);
    assert_eq!(empty.first_unsacked_len(), 1_000);
}

#[test]
fn dragonfly_compute_pipe_matches_the_block_formula() {
    let mut board = SackScoreboard::new(10_000, 2_000, 0, 1_000, 3, 16);
    board.insert_block(1_000, 3_000);
    assert_eq!(board.compute_pipe(), 8_000);
}

#[test]
fn dragonfly_snapshot_revert_restores_blocks() {
    let mut board = SackScoreboard::new(10_000, 0, 0, 1_000, 3, 16);
    board.insert_block(1_000, 3_000);
    let saved = board.snapshot();
    board.note_reneging();
    board.revert(saved, 0);
    assert_eq!(board.sacked, vec![(1_000, 3_000)]);
}

#[test]
fn sack_option_parse_and_dsack_classification() {
    let bytes = [5, 10, 0, 0, 0x03, 0xe8, 0, 0, 0x07, 0xd0];
    assert_eq!(parse_sack_option(&bytes), Some(vec![(1_000, 2_000)]));
    assert_eq!(parse_sack_option(&[5, 10, 0]), None);
    assert_eq!(ndsack_blocks(&[(-100, 0)], 0), 1);
    assert_eq!(ndsack_blocks(&[(1_000, 2_000), (500, 3_000)], 0), 2);
    assert_eq!(ndsack_blocks(&[(1_000, 2_000)], 0), 0);
}

#[test]
fn sack_report_history_and_fill_report() {
    let mut report = SackReport::new(8);
    report.note_block(2_000, 3_000, false);
    report.update_reported_history(2_000, 3_000);
    report.update_reported_history(3_500, 4_500);
    let blocks = report.fill_report(1_000, 4);
    assert_eq!(blocks, vec![(2_000, 3_000), (3_500, 4_500)]);
    assert!(!report.report_needed());
    report.note_block(1_000, 1_500, true);
    assert!(report.report_needed());
    report.cleanup();
    assert!(!report.report_needed());
}
