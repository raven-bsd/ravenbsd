// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Unit tests for the RACK misc allocator/free-list internals.

use super::*;
use crate::rack_types::{
    RACK_ACKED, RACK_APP_LIMITED, RACK_DEFERRED, RACK_HAD_PUSH, RACK_HAS_FIN, RACK_HAS_SYN,
    RACK_IN_GP_WIN, RACK_INITIAL_RTO, RACK_IS_PCM, RACK_MERGED, RACK_MUST_RXT, RACK_NUM_OF_RETRANS,
    RACK_OVERMAX, RACK_PMTU_CHG, RACK_RWND_COLLAPSED, RACK_SACK_PASSED, RACK_SENT_FP, RACK_SENT_SP,
    RACK_SENT_W_DSACK, RACK_SHUFFLED, RACK_STRADDLE, RACK_TLP, RACK_TO_REXT, RACK_WAS_ACKED,
    RACK_WAS_LOST, RACK_WAS_SACKPASS, RackSendmap,
};

#[test]
fn sendmap_flags_match_tcp_rack_h() {
    assert_eq!(RACK_ACKED, 0x000001);
    assert_eq!(RACK_TO_REXT, 0x000002);
    assert_eq!(RACK_DEFERRED, 0x000004);
    assert_eq!(RACK_OVERMAX, 0x000008);
    assert_eq!(RACK_SACK_PASSED, 0x000010);
    assert_eq!(RACK_WAS_SACKPASS, 0x000020);
    assert_eq!(RACK_HAS_FIN, 0x000040);
    assert_eq!(RACK_TLP, 0x000080);
    assert_eq!(RACK_RWND_COLLAPSED, 0x000100);
    assert_eq!(RACK_APP_LIMITED, 0x000200);
    assert_eq!(RACK_WAS_ACKED, 0x000400);
    assert_eq!(RACK_HAS_SYN, 0x000800);
    assert_eq!(RACK_SENT_W_DSACK, 0x001000);
    assert_eq!(RACK_SENT_SP, 0x002000);
    assert_eq!(RACK_SENT_FP, 0x004000);
    assert_eq!(RACK_HAD_PUSH, 0x008000);
    assert_eq!(RACK_MUST_RXT, 0x010000);
    assert_eq!(RACK_IN_GP_WIN, 0x020000);
    assert_eq!(RACK_SHUFFLED, 0x040000);
    assert_eq!(RACK_MERGED, 0x080000);
    assert_eq!(RACK_PMTU_CHG, 0x100000);
    assert_eq!(RACK_STRADDLE, 0x200000);
    assert_eq!(RACK_WAS_LOST, 0x400000);
    assert_eq!(RACK_IS_PCM, 0x800000);
    assert_eq!(RACK_NUM_OF_RETRANS, 3);
    assert_eq!(RACK_INITIAL_RTO, 1_000_000);
}

#[test]
fn sendmap_new_len_and_last_sent_use_the_donor_slots() {
    let map = RackSendmap::new(1_000, 2_000, 7);
    assert_eq!(map.len(), 1_000);
    assert!(!map.is_empty());
    assert_eq!(map.last_sent(), 7);
    assert_eq!(map.tim_lastsent.len(), RACK_NUM_OF_RETRANS);
    assert!(RackSendmap::new(5, 5, 1).is_empty());

    let mut retransmitted = map;
    retransmitted.rtr_cnt = 3;
    retransmitted.tim_lastsent = [1, 2, 3];
    assert_eq!(retransmitted.last_sent(), 3);
}

#[test]
fn allocation_reuses_the_hot_free_list() {
    let mut rack = RackMisc::new();
    let ids: alloc::vec::Vec<u64> = (0..5).map(|_| rack.rack_alloc().expect("alloc")).collect();
    for id in &ids {
        assert!(rack.rack_free(*id));
    }
    assert_eq!(rack.free_count, 5);
    let reused = rack.rack_alloc().expect("reuse");
    assert!(ids.contains(&reused));
}

#[test]
fn a_full_map_limit_reports_once() {
    let mut rack = RackMisc::new();
    rack.map_entries_limit = 1;
    let first = rack.rack_alloc_full_limit().expect("first");
    assert!(rack.rack_alloc_full_limit().is_none());
    assert!(rack.alloc_limit_reported);
    let _ = rack.rack_free(first);
}

#[test]
fn split_limit_marks_the_limit_type() {
    let mut rack = RackMisc::new();
    rack.split_limit = 1;
    let first = rack.rack_alloc_limit(1).expect("first");
    assert_eq!(rack.sendmaps.get(&first).expect("map").limit_type, 1);
    assert!(rack.rack_alloc_limit(1).is_none());
    assert_eq!(rack.split_allocs, 1);
}

#[test]
fn ctor_zeroes_the_zone_memory() {
    let mut memory = [0xaa; 16];
    assert_eq!(RackMisc::rack_ctor(&mut memory), 0);
    assert!(memory.iter().all(|byte| *byte == 0));
    RackMisc::rack_dtor(&mut memory);
}
