// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

use super::*;
use crate::Instant;
use crate::sack::SackHole;
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

#[test]
fn netbsd_holes_split_and_trim_from_sack_blocks() {
    let mut holes = SackHoles::new(16);
    holes.sack_option(1_000, 1_000, 11_000, &[(3_000, 4_000), (6_000, 7_000)]);
    assert_eq!(
        holes.holes,
        vec![
            SackHole {
                start: 1_000,
                end: 3_000,
                rxmit: 1_000
            },
            SackHole {
                start: 4_000,
                end: 6_000,
                rxmit: 4_000
            },
        ]
    );
    assert_eq!(holes.rcv_lastsack, 7_000);
    assert_eq!(holes.snd_fack, 7_000);
    let (hole, rexmit) = holes.output(1_000).unwrap();
    assert_eq!(hole.start, 1_000);
    assert_eq!(hole.end, 3_000);
    assert_eq!(rexmit, 0);
}

#[test]
fn netbsd_holes_split_a_middle_block() {
    let mut holes = SackHoles::new(16);
    holes.sack_option(1_000, 1_000, 11_000, &[(5_000, 6_000)]);
    assert_eq!(
        holes.holes,
        vec![SackHole {
            start: 1_000,
            end: 5_000,
            rxmit: 1_000
        }]
    );
    holes.holes[0].rxmit = 4_000;
    holes.sack_option(1_000, 1_000, 11_000, &[(2_000, 3_000)]);
    assert_eq!(
        holes.holes,
        vec![
            SackHole {
                start: 1_000,
                end: 2_000,
                rxmit: 2_000
            },
            SackHole {
                start: 3_000,
                end: 5_000,
                rxmit: 4_000
            },
        ]
    );
}

#[test]
fn netbsd_del_sackholes_trims_and_removes() {
    let mut holes = SackHoles::new(16);
    holes.sack_option(1_000, 1_000, 11_000, &[(3_000, 4_000), (6_000, 7_000)]);
    holes.del_sackholes(2_500, 1_000);
    assert_eq!(holes.holes[0].start, 2_500);
    holes.del_sackholes(4_000, 1_000);
    assert_eq!(
        holes.holes,
        vec![SackHole {
            start: 4_000,
            end: 6_000,
            rxmit: 4_000
        }]
    );
}

#[test]
fn netbsd_sack_output_advances_rxmit_and_counts_bytes() {
    let mut holes = SackHoles::new(16);
    holes.sack_option(1_000, 1_000, 11_000, &[(3_000, 4_000)]);
    assert_eq!(holes.holes[0].rxmit, 1_000);
    holes.holes[0].rxmit = 2_000;
    let (hole, bytes) = holes.output(1_000).unwrap();
    assert_eq!(hole.start, 1_000);
    assert_eq!(hole.rxmit, 2_000);
    assert_eq!(bytes, 1_000);
}

#[test]
fn netbsd_sack_adjust_moves_past_sacked_data() {
    let mut holes = SackHoles::new(16);
    holes.sack_option(1_000, 1_000, 11_000, &[(3_000, 4_000), (6_000, 7_000)]);
    assert_eq!(holes.adjust(3_500), 4_000);
    assert_eq!(holes.adjust(5_000), 5_000);
    assert_eq!(holes.adjust(7_000), 7_000);
}

#[test]
fn netbsd_dsack_and_block_count() {
    let mut holes = SackHoles::new(16);
    holes.new_dsack(900, 100);
    assert_eq!(holes.dsack, Some((900, 1_000)));
    assert_eq!(holes.numblks(0), 1);
    assert_eq!(holes.numblks(5), 4);
}

#[test]
fn netbsd_free_clears_the_scoreboard() {
    let mut holes = SackHoles::new(16);
    holes.sack_option(1_000, 1_000, 11_000, &[(3_000, 4_000)]);
    holes.free();
    assert!(holes.holes.is_empty());
    assert_eq!(holes.rcv_lastsack, 0);
    assert_eq!(holes.snd_fack, 0);
}

#[test]
fn netbsd_from_sacked_covers_the_tail() {
    let holes = SackHoles::from_sacked(1_000, 11_000, &[(3_000, 4_000), (6_000, 7_000)], 1_000, 16);
    assert_eq!(
        holes.holes,
        vec![
            SackHole {
                start: 1_000,
                end: 3_000,
                rxmit: 1_000
            },
            SackHole {
                start: 4_000,
                end: 6_000,
                rxmit: 4_000
            },
            SackHole {
                start: 7_000,
                end: 11_000,
                rxmit: 7_000
            },
        ]
    );
}
