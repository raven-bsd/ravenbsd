// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Named tests for the K360 canonical flag and sendmap transition matrix.

use std::collections::BTreeSet;

use raven_tcpip::{
    RACK_ACKED, RACK_APP_LIMITED, RACK_HAD_PUSH, RACK_HAS_FIN, RACK_HAS_SYN, RACK_MUST_RXT,
    RACK_NUM_OF_RETRANS, RACK_OVERMAX, RACK_PMTU_CHG, RACK_RWND_COLLAPSED, RACK_SACK_PASSED,
    RACK_TLP, RACK_WAS_LOST, RACK_WAS_SACKPASS, RackMisc, RackPipe, RackRetransmit, RackSack,
    RackSendMap, RackSendmap,
};

use super::{CANONICAL_NAMES, FIXTURE, check_matrix, parse_fixture};

#[test]
fn fixture_covers_every_canonical_constant() {
    let rows = parse_fixture(FIXTURE).expect("fixture parses");
    assert_eq!(rows.len(), CANONICAL_NAMES.len());
    let names: BTreeSet<&str> = rows.iter().map(|row| row.name.as_str()).collect();
    for name in CANONICAL_NAMES {
        assert!(names.contains(name), "fixture misses {name}");
    }
    let failures = check_matrix(FIXTURE).expect("matrix valid");
    assert!(failures.is_empty(), "{failures:?}");
}

#[test]
fn sendmap_types_and_lifecycle() {
    let mut entry = RackSendmap::new(100, 200, 7);
    let _: u32 = entry.start;
    let _: u32 = entry.end;
    let _: u32 = entry.flags;
    let _: u8 = entry.rtr_cnt;
    let _: u8 = entry.dupack;
    let _: u64 = entry.ack_arrival;
    let _: [u64; RACK_NUM_OF_RETRANS] = entry.tim_lastsent;
    assert_eq!(entry.len(), 100);
    assert!(!entry.is_empty());
    assert_eq!(entry.last_sent(), 7);
    assert_eq!(entry.tim_lastsent[1], 0);
    assert_eq!(entry.id, 0);
    assert!(entry.in_tmap);

    entry.rtr_cnt = 3;
    entry.tim_lastsent = [1, 2, 3];
    assert_eq!(entry.last_sent(), 3);
    assert_eq!(RackSendmap::with_id(42, 1, 2, 3).id, 42);
    assert!(RackSendmap::new(5, 5, 0).is_empty());
}

#[test]
fn every_flag_bit_is_unique_and_round_trips() {
    let flags: &[(&str, u32)] = &[
        ("RACK_ACKED", raven_tcpip::RACK_ACKED),
        ("RACK_TO_REXT", raven_tcpip::RACK_TO_REXT),
        ("RACK_DEFERRED", raven_tcpip::RACK_DEFERRED),
        ("RACK_OVERMAX", raven_tcpip::RACK_OVERMAX),
        ("RACK_SACK_PASSED", raven_tcpip::RACK_SACK_PASSED),
        ("RACK_WAS_SACKPASS", raven_tcpip::RACK_WAS_SACKPASS),
        ("RACK_HAS_FIN", raven_tcpip::RACK_HAS_FIN),
        ("RACK_TLP", raven_tcpip::RACK_TLP),
        ("RACK_RWND_COLLAPSED", raven_tcpip::RACK_RWND_COLLAPSED),
        ("RACK_APP_LIMITED", raven_tcpip::RACK_APP_LIMITED),
        ("RACK_WAS_ACKED", raven_tcpip::RACK_WAS_ACKED),
        ("RACK_HAS_SYN", raven_tcpip::RACK_HAS_SYN),
        ("RACK_SENT_W_DSACK", raven_tcpip::RACK_SENT_W_DSACK),
        ("RACK_SENT_SP", raven_tcpip::RACK_SENT_SP),
        ("RACK_SENT_FP", raven_tcpip::RACK_SENT_FP),
        ("RACK_HAD_PUSH", raven_tcpip::RACK_HAD_PUSH),
        ("RACK_MUST_RXT", raven_tcpip::RACK_MUST_RXT),
        ("RACK_IN_GP_WIN", raven_tcpip::RACK_IN_GP_WIN),
        ("RACK_SHUFFLED", raven_tcpip::RACK_SHUFFLED),
        ("RACK_MERGED", raven_tcpip::RACK_MERGED),
        ("RACK_PMTU_CHG", raven_tcpip::RACK_PMTU_CHG),
        ("RACK_STRADDLE", raven_tcpip::RACK_STRADDLE),
        ("RACK_WAS_LOST", raven_tcpip::RACK_WAS_LOST),
        ("RACK_IS_PCM", raven_tcpip::RACK_IS_PCM),
    ];
    for (index, (name, flag)) in flags.iter().enumerate() {
        assert_ne!(*flag, 0, "{name}");
        for (other_name, other) in &flags[index + 1..] {
            assert_ne!(flag, other, "{name} aliases {other_name}");
        }
        let mut entry = RackSendmap::new(0, 1, 0);
        entry.flags = *flag;
        assert_eq!(entry.flags, *flag, "{name}");
        entry.flags &= !*flag;
        assert_eq!(entry.flags, 0, "{name}");
    }
}

#[test]
fn misc_clone_and_merge_cover_the_edge_flags() {
    let mut misc = RackMisc::new();
    let left = misc.rack_alloc().expect("left");
    let right = misc.rack_alloc().expect("right");
    {
        let map = misc.sendmaps.get_mut(&left).expect("left map");
        map.start = 0;
        map.end = 100;
        map.flags = RACK_HAS_SYN | RACK_HAS_FIN | RACK_HAD_PUSH | RACK_ACKED | RACK_APP_LIMITED;
    }
    {
        let map = misc.sendmaps.get_mut(&right).expect("right map");
        map.start = 100;
        map.end = 200;
        map.flags = RACK_HAS_FIN | RACK_TLP | RACK_RWND_COLLAPSED | RACK_APP_LIMITED;
    }

    let clone = misc.rack_clone_rsm(left, 50).expect("clone");
    let cloned = misc.sendmaps.get(&clone).expect("clone map");
    assert_eq!(
        cloned.flags & (RACK_HAS_SYN | RACK_HAS_FIN | RACK_HAD_PUSH),
        0
    );
    assert_ne!(cloned.flags & RACK_ACKED, 0);
    assert_eq!(cloned.start, 50);
    assert_eq!(misc.sendmaps[&left].flags & RACK_HAS_FIN, 0);

    assert!(misc.rack_merge_rsm(left, right));
    let merged = misc.sendmaps.get(&left).expect("merged");
    assert_ne!(merged.flags & RACK_HAS_FIN, 0);
    assert_ne!(merged.flags & RACK_TLP, 0);
    assert_ne!(merged.flags & RACK_RWND_COLLAPSED, 0);
    assert_ne!(merged.flags & RACK_APP_LIMITED, 0);
    assert_eq!(merged.end, 200);
}

#[test]
fn misc_mtu_change_sets_must_rxt_and_pmtu() {
    let mut misc = RackMisc::new();
    misc.tp.t_maxseg = 1_200;
    let id = misc.rack_alloc().expect("map");
    misc.sendmaps.get_mut(&id).expect("entry").start = 1;
    misc.sendmaps.get_mut(&id).expect("entry").end = 1_001;

    assert!(misc.rack_mtu_change());
    let flags = misc.sendmaps[&id].flags;
    assert_ne!(flags & RACK_MUST_RXT, 0);
    assert_ne!(flags & RACK_PMTU_CHG, 0);

    misc.min_segs = 1_200;
    misc.sendmaps.get_mut(&id).expect("entry").flags &= !(RACK_MUST_RXT | RACK_PMTU_CHG);
    assert!(!misc.rack_mtu_change());
    assert_eq!(
        misc.sendmaps[&id].flags & (RACK_MUST_RXT | RACK_PMTU_CHG),
        0
    );
}

#[test]
fn retransmit_update_rsm_transitions_all_flags() {
    let mut rack = RackRetransmit::new(1_000);
    let id = rack.map.insert(0, 1_000, 1);
    {
        let entry = rack.map.find_id_mut(id).expect("entry");
        entry.flags =
            RACK_ACKED | RACK_SACK_PASSED | RACK_WAS_LOST | RACK_MUST_RXT | RACK_RWND_COLLAPSED;
    }
    rack.sacked = 1_000;
    rack.considered_lost = 1_000;
    rack.rack_update_rsm(id, 2, 0, 1_000);

    let entry = rack.map.find_id(id).expect("entry");
    assert_eq!(
        entry.flags
            & (RACK_ACKED | RACK_SACK_PASSED | RACK_WAS_LOST | RACK_MUST_RXT | RACK_RWND_COLLAPSED),
        0
    );
    assert_ne!(entry.flags & RACK_WAS_SACKPASS, 0);
    assert_eq!(entry.rtr_cnt, 2);
    assert_eq!(entry.act_rxt_cnt, 1);
    assert_eq!(entry.tim_lastsent[1], 2);
    assert_eq!(entry.rtr_bytes, 1_000);
    assert_eq!(rack.sacked, 0);
    assert_eq!(rack.considered_lost, 0);
}

#[test]
fn retransmit_overmax_and_edge_split() {
    let mut rack = RackRetransmit::new(1_000);
    let id = rack.map.insert(0, 1_000, 1);
    rack.rack_update_rsm(id, 1, 0, 1_000);
    rack.rack_update_rsm(id, 2, 0, 1_000);
    rack.rack_update_rsm(id, 3, 0, 1_000);
    let entry = rack.map.find_id(id).expect("entry");
    assert_eq!(entry.rtr_cnt, RACK_NUM_OF_RETRANS as u8);
    assert_ne!(entry.flags & RACK_OVERMAX, 0);

    let mut split_rack = RackRetransmit::new(1_000);
    let split_id = split_rack.map.insert(0, 1_000, 1);
    split_rack.map.find_id_mut(split_id).expect("entry").flags =
        RACK_HAS_SYN | RACK_HAS_FIN | RACK_HAD_PUSH;
    let result = split_rack.rack_update_entry(split_id, 2, 500, 0, 1_000);
    let tail_id = result.split.expect("tail");
    let left = split_rack.map.find_id(split_id).expect("left");
    let tail = split_rack.map.find_id(tail_id).expect("tail entry");
    assert_eq!(left.flags & (RACK_HAS_FIN | RACK_HAD_PUSH), 0);
    assert_ne!(left.flags & RACK_HAS_SYN, 0);
    assert_ne!(tail.flags & RACK_HAS_FIN, 0);
    assert_ne!(tail.flags & RACK_HAD_PUSH, 0);
    assert_eq!(tail.flags & RACK_HAS_SYN, 0);
    assert_eq!(left.end, 500);
    assert_eq!(tail.start, 500);
}

#[test]
fn retransmit_tlp_flag_toggles() {
    let mut rack = RackRetransmit::new(1_000);
    let id = rack.map.insert(1, 1_001, 1);
    assert!(rack.rack_fast_rsm_output(id, 1_000, true, 10, 40).is_ok());
    assert_ne!(rack.map.find_id(id).expect("entry").flags & RACK_TLP, 0);
    assert!(rack.rack_fast_rsm_output(id, 1_000, false, 20, 40).is_ok());
    assert_eq!(rack.map.find_id(id).expect("entry").flags & RACK_TLP, 0);
}

#[test]
fn sendmap_mark_acked_sets_the_acked_flag() {
    let mut map = RackSendMap::default();
    let id = map.insert(0, 1_000, 1);
    let mut sacked = 0;
    assert!(map.mark_acked(id, &mut sacked));
    assert_ne!(map.find_id(id).expect("entry").flags & RACK_ACKED, 0);
    assert_eq!(sacked, 1_000);
    assert!(!map.mark_acked(id, &mut sacked));
    assert_eq!(sacked, 1_000);
}

#[test]
fn sack_transitions_acked_sack_passed_was_lost() {
    let mut sack = RackSack::new();
    sack.stack.tp.snd_una = 0;
    sack.stack.tp.snd_max = 1_000;
    sack.threshold_us = 100;
    sack.insert_segment(0, 100, 0, 1);
    sack.insert_segment(100, 200, 0, 1);
    sack.insert_segment(200, 300, 0, 1);

    assert_eq!(sack.rack_proc_sack_blk(50, 150, 1_000, 1_460), 100);
    assert_ne!(sack.sendmap[&50].flags & RACK_ACKED, 0);
    assert_ne!(sack.sendmap[&100].flags & RACK_ACKED, 0);
    assert_ne!(sack.sendmap[&0].flags & RACK_WAS_LOST, 0);

    let mut passed = RackSack::new();
    passed.threshold_us = 100;
    passed.insert_segment(0, 100, 0, 1);
    passed.insert_segment(100, 200, 0, 1);
    passed.insert_segment(200, 300, 0, 1);
    assert_eq!(passed.rack_log_sack_passed(300, 100), 300);
    for key in [0, 100, 200] {
        assert_ne!(passed.sendmap[&key].flags & RACK_SACK_PASSED, 0);
        assert_ne!(passed.sendmap[&key].flags & RACK_WAS_LOST, 0);
    }
}

#[test]
fn pipe_marks_and_clears_was_lost() {
    let mut pipe = RackPipe::new();
    pipe.rack_rtt = 1_000;
    pipe.push_sendmap(RackSendmap::new(1_000, 2_000, 1_000));
    pipe.sendmap_mut(0).expect("entry").flags = RACK_SACK_PASSED;
    pipe.rack_mark_lost(0, 2_000);
    assert_ne!(pipe.sendmap()[0].flags & RACK_WAS_LOST, 0);
    pipe.rack_mark_nolonger_lost(0);
    assert_eq!(pipe.sendmap()[0].flags & RACK_WAS_LOST, 0);
}
