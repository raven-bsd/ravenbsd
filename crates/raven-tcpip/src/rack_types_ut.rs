// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Cross-module tests for the K349 canonical RACK flags/sendmap.

use super::*;
use crate::{rack_misc, rack_pipe, rack_retransmit, rack_sack};

#[test]
fn canonical_flags_match_tcp_rack_h() {
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
    assert_eq!(RACK_RTO_MIN, 30_000);
    assert_eq!(RACK_RTO_MAX, 4_000_000);
    assert_eq!(RACK_TLP_MIN, 10_000);
    assert_eq!(RACK_DELAYED_ACK_TIME, 40_000);
    assert_eq!(RACK_PRR_ADDBACKMAX, 2);
    assert_eq!(PACE_MAX_IP_BYTES, 65_536);
    assert_eq!(TF_FASTRECOVERY, 0x0010_0000);
    assert_eq!(TF_CONGRECOVERY, 0x2000_0000);
}

#[test]
fn every_rack_module_uses_the_canonical_sendmap() {
    let mut misc = rack_misc::RackMisc::new();
    misc.sendmaps.insert(1, RackSendmap::new(1, 2, 3));
    let _: &RackSendmap = misc.sendmaps.get(&1).expect("misc entry");

    let mut pipe = rack_pipe::RackPipe::new();
    pipe.push_sendmap(RackSendmap::new(10, 20, 30));
    let _: &[RackSendmap] = pipe.sendmap();

    let mut map = rack_retransmit::RackSendMap::default();
    let id = map.insert(100, 200, 300);
    let _: &RackSendmap = map.find_id(id).expect("retransmit entry");

    let mut sack = rack_sack::RackSack::new();
    sack.insert_segment(0, 100, 5, 1);
    let _: &RackSendmap = &sack.sendmap[&0];
}

#[test]
fn rack_sack_uses_the_canonical_flag_values() {
    let mut sack = rack_sack::RackSack::new();
    sack.threshold_us = 100;
    for (start, end) in [(0, 100), (100, 200), (200, 300)] {
        sack.insert_segment(start, end, 0, 1);
    }
    for key in [0, 100, 200] {
        sack.sendmap.get_mut(&key).expect("entry").flags |= RACK_WAS_LOST;
    }
    assert_eq!(sack.rack_log_sack_passed(300, 200), 0);
}
