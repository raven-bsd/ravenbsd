// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

use super::*;
use crate::Instant;
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
