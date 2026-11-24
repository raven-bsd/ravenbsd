// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Named tests for the K336 SACK/DSACK/reneging donor group.

use alloc::vec;

use super::*;

#[test]
fn rack_log_dsack_event_records_the_k314_words() {
    let mut sack = RackSack::new();
    sack.rack_log_dsack_event(5, 1, 2, 3);
    assert_eq!(sack.last_event, Some(RACK_DSACK_HANDLING));
    assert_eq!(sack.last_dsack_log, Some([5, 1, 2, 3]));
    sack.rack_log_dsack_event(7, 10, 20, 30);
    assert_eq!(sack.last_dsack_log, Some([7, 10, 20, 30]));
}

#[test]
fn sack_filter_blocks_are_merged_and_cleared() {
    let mut sack = RackSack::new();
    assert_eq!(
        sack.sack_filter_blks(&[(0, 100), (100, 200), (300, 400)], 50),
        2
    );
    assert_eq!(sack.sack_filter, vec![(50, 200), (300, 400)]);
    sack.sack_filter_clear(150);
    assert!(sack.sack_filter.is_empty());
}

#[test]
fn rack_proc_sack_blk_splits_merges_and_marks_acked() {
    let mut sack = RackSack::new();
    sack.stack.tp.snd_una = 0;
    sack.stack.tp.snd_max = 1_000;
    sack.threshold_us = 100;
    sack.insert_segment(0, 100, 0, 1);
    sack.insert_segment(100, 200, 0, 1);
    sack.insert_segment(200, 300, 0, 1);

    assert_eq!(sack.rack_proc_sack_blk(50, 150, 1_000, 1_460), 100);
    assert_eq!(sack.sacked_ranges, vec![(50, 150)]);
    assert_eq!(sack.sacked_bytes(), 100);
    assert_eq!(sack.stack.sacked, 100);
    assert_eq!(sack.sendmap[&0].end, 50);
    assert_eq!(sack.sendmap[&50].flags & RACK_ACKED, RACK_ACKED);
    assert_eq!(sack.sendmap[&100].flags & RACK_ACKED, RACK_ACKED);
    assert_eq!(sack.sendmap[&150].flags & RACK_ACKED, 0);
    assert_eq!(sack.sendmap[&200].flags & RACK_ACKED, 0);
    // The reverse walk from the ACKed block marks the one older range lost.
    assert_eq!(sack.sendmap[&0].flags & RACK_WAS_LOST, RACK_WAS_LOST);
    assert_eq!(sack.stack.considered_lost, 50);
}

#[test]
fn rack_log_sack_passed_marks_only_older_ranges_and_stops() {
    let mut sack = RackSack::new();
    sack.threshold_us = 100;
    sack.insert_segment(0, 100, 0, 1);
    sack.insert_segment(100, 200, 0, 1);
    sack.insert_segment(200, 300, 0, 1);

    assert_eq!(sack.rack_log_sack_passed(300, 100), 300);
    assert_eq!(sack.stack.considered_lost, 300);
    for key in [0, 100, 200] {
        assert_eq!(sack.sendmap[&key].flags & RACK_WAS_LOST, RACK_WAS_LOST);
        assert_eq!(
            sack.sendmap[&key].flags & RACK_SACK_PASSED,
            RACK_SACK_PASSED
        );
    }
    // The first already-passed range stops the reverse walk.
    assert_eq!(sack.rack_log_sack_passed(300, 200), 0);
}

#[test]
fn rack_peer_reneges_clears_ranges_below_the_cumack() {
    let mut sack = RackSack::new();
    sack.insert_segment(0, 100, 0, 1);
    sack.insert_segment(200, 300, 0, 1);
    sack.sendmap.get_mut(&0).expect("first").flags |= RACK_ACKED;
    sack.sendmap.get_mut(&200).expect("third").flags |= RACK_ACKED;
    sack.sacked_ranges = vec![(0, 100), (200, 300)];
    sack.stack.sacked = 200;
    sack.sack_filter = vec![(0, 100), (200, 300)];

    assert_eq!(sack.rack_peer_reneges(150), 100);
    assert_eq!(sack.sacked_ranges, vec![(200, 300)]);
    assert_eq!(sack.stack.sacked, 100);
    assert_eq!(sack.sendmap[&0].flags & RACK_ACKED, 0);
    assert_eq!(sack.sendmap[&200].flags & RACK_ACKED, RACK_ACKED);
    assert!(sack.sack_filter.is_empty());
}

#[test]
fn rack_process_to_cumack_frees_trims_and_advances() {
    let mut sack = RackSack::new();
    sack.stack.tp.snd_una = 0;
    sack.stack.tp.snd_max = 300;
    sack.insert_segment(0, 100, 0, 1);
    sack.insert_segment(100, 200, 0, 1);
    sack.insert_segment(200, 300, 0, 1);
    sack.sacked_ranges = vec![(0, 100), (200, 300)];
    sack.sack_filter = vec![(0, 100)];

    assert_eq!(sack.rack_process_to_cumack(150, 1_000, 500), 150);
    assert_eq!(sack.stack.tp.snd_una, 150);
    assert_eq!(sack.stack.sacked, 100);
    assert_eq!(sack.sacked_ranges, vec![(200, 300)]);
    assert_eq!(sack.last_cumack_advance, 500);
    assert!(sack.stack.wanted_output);
    assert!(!sack.sendmap.contains_key(&0));
    assert_eq!(sack.sendmap[&150].start, 150);
    assert_eq!(sack.sendmap[&150].end, 200);
    assert_eq!(sack.sendmap[&200].end, 300);
}

#[test]
fn rack_note_dsack_counts_rounds_and_requests_spurious_recovery() {
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

    let mut sent = RackSack::new();
    sent.last_sent_tlp = Some((300, 50));
    assert!(sent.rack_note_dsack(320, 340));
    assert_eq!(sent.dsack_byte_cnt, 20);
}

#[test]
fn tlp_markers_age_out_after_the_cumack_passes() {
    let mut sack = RackSack::new();
    sack.last_tlp_acked = Some((100, 200));
    sack.rack_process_to_cumack(200, 1_000, 1);
    assert!(sack.last_tlp_past_cumack);
    sack.rack_process_to_cumack(50, 1_000, 2);
    assert!(sack.last_tlp_acked.is_none());

    sack.last_sent_tlp = Some((300, 50));
    sack.rack_process_to_cumack(300, 1_000, 3);
    assert!(sack.last_sent_tlp_past_cumack);
    sack.rack_process_to_cumack(100, 1_000, 4);
    assert!(sack.last_sent_tlp.is_none());
}
