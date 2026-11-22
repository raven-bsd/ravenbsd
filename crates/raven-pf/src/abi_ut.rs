// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host tests for the pf.h ABI surface.

use super::*;
use crate::address::IpAddr;

#[test]
fn the_action_and_operation_enums_match_the_donor() {
    assert_eq!(PF_PASS, 0);
    assert_eq!(PF_DROP, 1);
    assert_eq!(PF_RT, 14);
    assert_eq!(PF_OP_NONE, 0);
    assert_eq!(PF_OP_IRG, 1);
    assert_eq!(PF_OP_EQ, 2);
    assert_eq!(PF_OP_NE, 3);
    assert_eq!(PF_OP_LT, 4);
    assert_eq!(PF_OP_LE, 5);
    assert_eq!(PF_OP_GT, 6);
    assert_eq!(PF_OP_GE, 7);
    assert_eq!(PF_OP_XRG, 8);
    assert_eq!(PF_OP_RRG, 9);
}

#[test]
fn the_pool_and_log_flags_match_the_donor() {
    assert_eq!(PF_POOL_NONE, 0);
    assert_eq!(PF_POOL_BITMASK, 1);
    assert_eq!(PF_POOL_RANDOM, 2);
    assert_eq!(PF_POOL_SRCHASH, 3);
    assert_eq!(PF_POOL_ROUNDROBIN, 4);
    assert_eq!(PF_POOL_STICKYADDR, 0x20);
    assert_eq!(PF_LOG, 0x01);
    assert_eq!(PF_LOG_ALL, 0x02);
    assert_eq!(PF_LOG_USER, 0x04);
    assert_eq!(PF_LOG_FORCE, 0x08);
    assert_eq!(PF_LOG_MATCHES, 0x10);
}

#[test]
fn the_timeout_indices_and_defaults_match_the_donor() {
    assert_eq!(PFTM_TCP_FIRST_PACKET, 0);
    assert_eq!(PFTM_TCP_ESTABLISHED, 2);
    assert_eq!(PFTM_UDP_FIRST_PACKET, 6);
    assert_eq!(PFTM_ICMP_ERROR_REPLY, 10);
    assert_eq!(PFTM_OTHER_MULTIPLE, 13);
    assert_eq!(PFTM_FRAG, 14);
    assert_eq!(PFTM_INTERVAL, 15);
    assert_eq!(PFTM_TS_DIFF, 19);
    assert_eq!(PFTM_OLD_MAX, 20);
    assert_eq!(PFTM_MAX, 25);
    assert_eq!(PFTM_UNLINKED, 27);
    assert_eq!(PFTM_DEFAULT[PFTM_TCP_FIRST_PACKET], 120);
    assert_eq!(PFTM_DEFAULT[PFTM_TCP_OPENING], 30);
    assert_eq!(PFTM_DEFAULT[PFTM_TCP_ESTABLISHED], 24 * 60 * 60);
    assert_eq!(PFTM_DEFAULT[PFTM_TCP_CLOSING], 15 * 60);
    assert_eq!(PFTM_DEFAULT[PFTM_TCP_FIN_WAIT], 45);
    assert_eq!(PFTM_DEFAULT[PFTM_TCP_CLOSED], 90);
    assert_eq!(PFTM_DEFAULT[PFTM_UDP_FIRST_PACKET], 60);
    assert_eq!(PFTM_DEFAULT[PFTM_UDP_SINGLE], 30);
    assert_eq!(PFTM_DEFAULT[PFTM_UDP_MULTIPLE], 60);
    assert_eq!(PFTM_DEFAULT[PFTM_ICMP_FIRST_PACKET], 20);
    assert_eq!(PFTM_DEFAULT[PFTM_ICMP_ERROR_REPLY], 10);
    assert_eq!(PFTM_DEFAULT[PFTM_OTHER_FIRST_PACKET], 60);
    assert_eq!(PFTM_DEFAULT[PFTM_OTHER_SINGLE], 30);
    assert_eq!(PFTM_DEFAULT[PFTM_OTHER_MULTIPLE], 60);
    assert_eq!(PFTM_DEFAULT[PFTM_FRAG], 60);
    assert_eq!(PFTM_DEFAULT[PFTM_INTERVAL], 10);
    assert_eq!(PFTM_DEFAULT[PFTM_SRC_NODE], 0);
    assert_eq!(PFTM_DEFAULT[PFTM_TS_DIFF], 30);
    assert_eq!(PFTM_DEFAULT.len(), PFTM_OLD_MAX);
}

#[test]
fn the_drop_reasons_match_the_donor() {
    assert_eq!(PFRES_MATCH, 0);
    assert_eq!(PFRES_BADOFF, 1);
    assert_eq!(PFRES_FRAG, 2);
    assert_eq!(PFRES_SHORT, 3);
    assert_eq!(PFRES_NORM, 4);
    assert_eq!(PFRES_MEMORY, 5);
    assert_eq!(PFRES_TS, 6);
    assert_eq!(PFRES_CONGEST, 7);
    assert_eq!(PFRES_IPOPTIONS, 8);
    assert_eq!(PFRES_PROTCKSUM, 9);
    assert_eq!(PFRES_BADSTATE, 10);
    assert_eq!(PFRES_STATEINS, 11);
    assert_eq!(PFRES_MAXSTATES, 12);
    assert_eq!(PFRES_SRCLIMIT, 13);
    assert_eq!(PFRES_SYNPROXY, 14);
    assert_eq!(PFRES_MAPFAILED, 15);
    assert_eq!(PFRES_TRANSLATE, 16);
    assert_eq!(PFRES_MAX, 17);
    assert_eq!(PFRES_NAMES.len(), PFRES_MAX);
    assert_eq!(PFRES_NAMES[PFRES_MATCH], "match");
    assert_eq!(PFRES_NAMES[PFRES_MAXSTATES], "state-limit");
    assert_eq!(PFRES_NAMES[PFRES_MAPFAILED], "map-failed");
    assert_eq!(PFRES_NAMES[PFRES_TRANSLATE], "translate");
}

#[test]
fn pf_addr_round_trips_both_families() {
    let v4 = IpAddr::parse("203.0.113.7").unwrap();
    let stored = PfAddr::from_ip(v4);
    assert_eq!(&stored.addr[..4], &[203, 0, 113, 7]);
    assert_eq!(
        stored.as_v4(),
        crate::address::Ipv4Addr::new(203, 0, 113, 7)
    );

    let v6 = IpAddr::parse("2001:db8::1").unwrap();
    let stored = PfAddr::from_ip(v6);
    assert_eq!(stored.as_v6().to_string(), "2001:db8::1");
    assert_eq!(core::mem::size_of::<PfAddr>(), 16);
}

#[test]
fn the_status_snapshot_defaults_to_stopped_and_zeroed() {
    let status = PfStatus::default();
    assert!(!status.running);
    assert_eq!(status.counters, [0; PFRES_MAX]);
    assert_eq!(status.states, 0);
}
