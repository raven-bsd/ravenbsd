// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! K334: every RACK init/alloc/sockopt/misc donor function.

use raven_tcpip::{
    Errno, IPPROTO_TCP, MiscQuery, PRUS_OOB, RackMisc, RackSockopt, SockoptDir,
    TCP_QUERY_RACK_TIMES, TCP_QUERY_SENDMAP, TCP_QUERY_TIMERS_UP, TCP_RACK_HI_BETA,
    TCP_RACK_PACE_MAX_SEG, TCP_RACK_PACING_DIVISOR, TCP_RACK_PROFILE, TCP_RACK_SET_RXT_OPTIONS,
    TCPS_ESTABLISHED, TF_SACK_PERMIT,
};

fn rack() -> RackMisc {
    let mut rack = RackMisc::new();
    rack.tp.snd_una = 100;
    rack.tp.snd_nxt = 200;
    rack.tp.snd_max = 200;
    rack.tp.snd_cwnd = 4_000;
    rack.iss = 50;
    rack.initialized = true;
    rack
}

fn sockopt(dir: SockoptDir, name: i32, value: u32) -> RackSockopt {
    RackSockopt {
        dir,
        level: IPPROTO_TCP,
        name,
        value,
    }
}

#[test]
fn rack_alloc_returns_and_reuses_a_map() {
    let mut rack = rack();
    let ids: Vec<u64> = (0..5).map(|_| rack.rack_alloc().expect("alloc")).collect();
    assert_eq!(rack.maps_alloced, 5);
    for id in &ids {
        assert!(rack.rack_free(*id));
    }
    assert_eq!(rack.free_count, 5);
    let reused = rack.rack_alloc().expect("reuse");
    assert!(ids.contains(&reused));
}

#[test]
fn rack_alloc_full_limit_stops_at_the_limit() {
    let mut rack = rack();
    rack.map_entries_limit = 1;
    assert!(rack.rack_alloc_full_limit().is_some());
    assert!(rack.rack_alloc_full_limit().is_none());
    assert!(rack.alloc_limit_reported);
}

#[test]
fn rack_alloc_limit_marks_the_split_allocation() {
    let mut rack = rack();
    rack.split_limit = 1;
    let map = rack.rack_alloc_limit(1).expect("split");
    assert_eq!(rack.sendmaps.get(&map).expect("map").limit_type, 1);
    assert!(rack.rack_alloc_limit(1).is_none());
    assert_eq!(rack.split_allocs, 1);
}

#[test]
fn rack_free_trim_returns_to_the_hot_cache() {
    let mut rack = rack();
    let ids: Vec<u64> = (0..10).map(|_| rack.rack_alloc().expect("alloc")).collect();
    for id in ids {
        assert!(rack.rack_free(id));
    }
    assert!(rack.free_count <= raven_tcpip::RACK_FREE_CNT_MAX);
    rack.rack_free_trim();
    assert_eq!(rack.free_count, raven_tcpip::RACK_FREE_CACHE);
    assert_eq!(rack.maps_alloced as usize, rack.free_count);
}

#[test]
fn rack_free_clears_the_map_and_keeps_the_count() {
    let mut rack = rack();
    let id = rack.rack_alloc().expect("alloc");
    if let Some(map) = rack.sendmaps.get_mut(&id) {
        map.start = 10;
        map.flags = raven_tcpip::RACK_HAS_FIN;
    }
    assert!(rack.rack_free(id));
    let map = rack.sendmaps.get(&id).expect("freed map");
    assert_eq!(*map, raven_tcpip::RackSendmap::default());
    assert_eq!(rack.free_count, 1);
}

#[test]
fn rack_clone_rsm_copies_the_left_edge_and_flags() {
    let mut rack = rack();
    let source = rack.rack_alloc().expect("source");
    if let Some(map) = rack.sendmaps.get_mut(&source) {
        map.start = 100;
        map.end = 200;
        map.flags = raven_tcpip::RACK_HAS_FIN | raven_tcpip::RACK_APP_LIMITED;
        map.dupack = 3;
        map.rtr_cnt = 2;
        map.tim_lastsent[0] = 7;
    }
    let clone = rack.rack_clone_rsm(source, 150).expect("clone");
    let clone_map = rack.sendmaps.get(&clone).expect("clone map");
    assert_eq!(clone_map.start, 150);
    assert_eq!(clone_map.end, 200);
    assert_eq!(clone_map.dupack, 3);
    assert_eq!(clone_map.tim_lastsent[0], 7);
    assert_eq!(clone_map.flags & raven_tcpip::RACK_HAS_FIN, 0);
}

#[test]
fn rack_merge_rsm_extends_the_left_map_and_frees_the_right() {
    let mut rack = rack();
    let left = rack.rack_alloc().expect("left");
    let right = rack.rack_alloc().expect("right");
    if let Some(map) = rack.sendmaps.get_mut(&left) {
        map.start = 100;
        map.end = 150;
        map.rtr_bytes = 10;
        map.dupack = 1;
    }
    if let Some(map) = rack.sendmaps.get_mut(&right) {
        map.start = 150;
        map.end = 220;
        map.rtr_bytes = 20;
        map.dupack = 4;
        map.flags = raven_tcpip::RACK_HAS_FIN;
    }
    assert!(rack.rack_merge_rsm(left, right));
    let map = rack.sendmaps.get(&left).expect("left map");
    assert_eq!(map.end, 220);
    assert_eq!(map.rtr_bytes, 30);
    assert_eq!(map.dupack, 4);
    assert_eq!(
        map.flags & raven_tcpip::RACK_HAS_FIN,
        raven_tcpip::RACK_HAS_FIN
    );
    assert_eq!(rack.free_count, 1);
}

#[test]
fn rack_init_fsb_block_sets_the_header_and_window() {
    let mut rack = rack();
    rack.rack_init_fsb_block();
    assert!(rack.fsb_inited);
    assert_eq!(rack.fsb.header_len, 40);
    let mut v6 = RackMisc::new();
    v6.ipv6 = true;
    v6.tunnel = true;
    v6.rack_init_fsb_block();
    assert_eq!(v6.fsb.header_len, 68);
}

#[test]
fn rack_init_fsb_allocates_and_fini_releases() {
    let mut rack = rack();
    rack.rack_init_fsb().expect("fsb");
    assert!(rack.fsb.mbuf.is_some());
    let freed = rack.rack_fini();
    assert_eq!(freed, 0);
    assert!(rack.fsb.mbuf.is_none());
    assert!(!rack.initialized);
}

#[test]
fn rack_deferred_init_copies_the_send_state() {
    let mut rack = rack();
    rack.snd_wnd = 4_096;
    rack.tp.snd_cwnd = 2_048;
    rack.rack_deferred_init();
    assert!(rack.deferred_inited);
    assert_eq!(rack.roundends, 200);
    assert_eq!(rack.high_rwnd, 4_096);
    assert_eq!(rack.cwnd_to_use, 2_048);
}

#[test]
fn rack_init_retransmit_value_selects_the_three_modes() {
    let mut rack = rack();
    rack.rack_init_retransmit_value(1);
    assert!(rack.full_size_rxt && !rack.shape_rxt_to_pacing_min);
    rack.rack_init_retransmit_value(2);
    assert!(!rack.full_size_rxt && rack.shape_rxt_to_pacing_min);
    rack.rack_init_retransmit_value(0);
    assert!(!rack.full_size_rxt && !rack.shape_rxt_to_pacing_min);
}

#[test]
fn rack_init_outstanding_builds_the_current_flight() {
    let mut rack = rack();
    let id = rack.rack_init_outstanding().expect("outstanding");
    let map = rack.sendmaps.get(&id).expect("map");
    assert_eq!(map.start, 100);
    assert_eq!(map.end, 200);
    assert!(map.no_rtt_allowed);
    assert!(map.in_tmap);

    rack.tp.snd_una = 50;
    let syn = rack.rack_init_outstanding().expect("syn outstanding");
    let map = rack.sendmaps.get(&syn).expect("syn map");
    assert_eq!(map.start, 50);
    assert_eq!(map.end, 50 + (200 - 50));
    assert_eq!(
        map.flags & raven_tcpip::RACK_HAS_SYN,
        raven_tcpip::RACK_HAS_SYN
    );
}

#[test]
fn rack_init_and_fini_lifecycle() {
    let mut rack = RackMisc::new();
    rack.tp.t_state = TCPS_ESTABLISHED;
    rack.rack_init().expect("init");
    assert!(rack.initialized);
    assert!(rack.fsb.mbuf.is_some());
    assert!(rack.deferred_inited);
    assert_eq!(rack.rack_fini(), 0);
    assert!(!rack.initialized);
}

#[test]
fn rack_mtu_change_marks_every_sendmap() {
    let mut rack = rack();
    rack.min_segs = 1_200;
    rack.tp.t_maxseg = 1_460;
    let id = rack.rack_alloc().expect("map");
    assert!(rack.rack_mtu_change());
    let map = rack.sendmaps.get(&id).expect("map");
    assert_eq!(
        map.flags & (raven_tcpip::RACK_MUST_RXT | raven_tcpip::RACK_PMTU_CHG),
        raven_tcpip::RACK_MUST_RXT | raven_tcpip::RACK_PMTU_CHG
    );
    assert_eq!(rack.tp.snd_nxt, rack.tp.snd_max);
    rack.min_segs = rack.tp.t_maxseg;
    assert!(!rack.rack_mtu_change());
}

#[test]
fn rack_sndbuf_autoscale_grows_the_send_buffer() {
    let mut rack = rack();
    rack.os.snd.hiwat = 1_000;
    rack.os.snd.ccc = 900;
    rack.snd_wnd = 2_000;
    rack.snd_cwnd = 2_000;
    rack.autosndbuf_inc = 100;
    rack.autosndbuf_max = 5_000;
    assert!(rack.rack_sndbuf_autoscale());
    assert_eq!(rack.os.snd.hiwat, 2_000);
}

#[test]
fn rack_set_profile_switches_profiles() {
    let mut rack = rack();
    rack.rack_set_profile(1).expect("profile 1");
    assert!(rack.dgp_on && rack.always_pace && !rack.pace_to_cwnd);
    rack.rack_set_profile(6).expect("profile 6");
    assert!(rack.pace_to_cwnd);
    rack.rack_set_profile(0).expect("profile 0");
    assert!(!rack.dgp_on && !rack.always_pace);
    assert_eq!(rack.rack_set_profile(9), Err(Errno::EINVAL));
}

#[test]
fn rack_add_deferred_option_is_bounded() {
    let mut rack = rack();
    for index in 0..raven_tcpip::RACK_DEFERRED_MAX {
        assert!(rack.rack_add_deferred_option(TCP_RACK_HI_BETA, index as u64));
    }
    assert!(!rack.rack_add_deferred_option(TCP_RACK_HI_BETA, 1));
}

#[test]
fn rack_stack_information_reports_the_counters() {
    let mut rack = rack();
    rack.bytes_transmitted = 1234;
    rack.bytes_retransmitted = 56;
    let info = rack.rack_stack_information();
    assert_eq!(info.bytes_transmitted, 1234);
    assert_eq!(info.bytes_retransmitted, 56);
}

#[test]
fn rack_inherit_copies_the_option_state() {
    let mut parent = rack();
    parent.full_size_rxt = true;
    parent.dsack_use = true;
    parent.hi_beta = true;
    parent.req_measurements = 4;
    parent.pacing_divisor = 300;
    let mut child = rack();
    let changed = child.rack_inherit(&parent);
    assert!(changed >= 5);
    assert!(child.full_size_rxt);
    assert!(child.dsack_use);
    assert!(child.hi_beta);
    assert_eq!(child.req_measurements, 4);
    assert_eq!(child.pacing_divisor, 300);
}

#[test]
fn rack_apply_deferred_options_applies_and_clears() {
    let mut rack = rack();
    assert!(rack.rack_add_deferred_option(TCP_RACK_HI_BETA, 1));
    assert_eq!(rack.rack_apply_deferred_options(), 1);
    assert!(rack.hi_beta);
    assert!(rack.deferred.is_empty());
}

#[test]
fn rack_set_sockopt_sets_directly() {
    let mut rack = rack();
    rack.rack_set_sockopt(&sockopt(SockoptDir::Set, TCP_RACK_HI_BETA, 1))
        .expect("set");
    assert!(rack.hi_beta);
    rack.rack_set_sockopt(&sockopt(SockoptDir::Set, TCP_RACK_PACING_DIVISOR, 0))
        .expect("divisor");
    assert_eq!(
        rack.pacing_divisor,
        raven_tcpip::RACK_DEFAULT_PACING_DIVISOR
    );
}

#[test]
fn rack_set_sockopt_defers_until_apply() {
    let mut rack = rack();
    rack.rack_set_sockopt(&sockopt(SockoptDir::Set, TCP_RACK_PROFILE, 1))
        .expect("profile");
    rack.rack_set_sockopt(&sockopt(SockoptDir::Set, raven_tcpip::TCP_DEFER_OPTIONS, 1))
        .expect("defer");
    rack.rack_set_sockopt(&sockopt(SockoptDir::Set, TCP_RACK_HI_BETA, 1))
        .expect("deferred hi beta");
    assert!(!rack.deferred.is_empty());
    assert!(!rack.hi_beta);
    assert_eq!(rack.rack_apply_deferred_options(), 1);
    assert!(rack.hi_beta);
}

#[test]
fn rack_fill_info_snapshots_the_tcp_state() {
    let mut rack = rack();
    rack.tp.t_state = TCPS_ESTABLISHED;
    rack.tp.t_srtt = 55;
    rack.tp.t_rttvar = 6;
    rack.tp.t_rxtcur = 70;
    rack.tp.snd_ssthresh = 900;
    rack.tp.t_flags = TF_SACK_PERMIT;
    let info = rack.rack_fill_info();
    assert_eq!(info.state, TCPS_ESTABLISHED);
    assert_eq!(info.rtt, 55);
    assert_eq!(info.rttvar, 6);
    assert_eq!(info.rto, 70);
    assert_eq!(info.snd_ssthresh, 900);
    assert_eq!(info.snd_nxt, 200);
    assert_ne!(info.options & 1, 0);
}

#[test]
fn rack_get_sockopt_round_trips_set_values() {
    let mut rack = rack();
    rack.rack_set_sockopt(&sockopt(SockoptDir::Set, TCP_RACK_HI_BETA, 1))
        .expect("set");
    rack.rack_set_sockopt(&sockopt(SockoptDir::Set, TCP_RACK_PACE_MAX_SEG, 7))
        .expect("max");
    assert_eq!(
        rack.rack_get_sockopt(&sockopt(SockoptDir::Get, TCP_RACK_HI_BETA, 0)),
        Ok(Some(1))
    );
    assert_eq!(
        rack.rack_get_sockopt(&sockopt(SockoptDir::Get, TCP_RACK_PACE_MAX_SEG, 0)),
        Ok(Some(7))
    );
    assert_eq!(
        rack.rack_get_sockopt(&sockopt(SockoptDir::Get, TCP_RACK_PROFILE, 0)),
        Err(Errno::EINVAL)
    );
}

#[test]
fn rack_ctor_and_dtor_are_safe() {
    let mut memory = [0xaa; 32];
    assert_eq!(RackMisc::rack_ctor(&mut memory), 0);
    assert!(memory.iter().all(|byte| *byte == 0));
    RackMisc::rack_dtor(&mut memory);
}

#[test]
fn rack_chg_query_fills_the_sendmap_and_timer_views() {
    let mut rack = rack();
    let id = rack.rack_alloc().expect("map");
    if let Some(map) = rack.sendmaps.get_mut(&id) {
        map.start = 100;
        map.end = 180;
        map.rtr_cnt = 2;
        map.dupack = 3;
        map.flags = raven_tcpip::RACK_HAS_FIN;
        map.tim_lastsent[0] = 9;
    }
    let mut query = MiscQuery {
        req: TCP_QUERY_SENDMAP,
        req_param: 120,
        ..MiscQuery::default()
    };
    assert_eq!(rack.rack_chg_query(&mut query), 1);
    assert_eq!(query.sendmap_start, 100);
    assert_eq!(query.sendmap_end, 180);
    assert_eq!(query.sendmap_send_cnt, 2);
    assert_eq!(query.sendmap_dupacks, 3);
    assert_eq!(query.sendmap_time[0], 9);

    let mut times = MiscQuery {
        req: TCP_QUERY_RACK_TIMES,
        ..MiscQuery::default()
    };
    assert_eq!(rack.rack_chg_query(&mut times), 1);
    assert_eq!(times.rack_rtt, rack.tp.t_srtt);

    let mut timers = MiscQuery {
        req: TCP_QUERY_TIMERS_UP,
        ..MiscQuery::default()
    };
    assert_eq!(rack.rack_chg_query(&mut timers), 0);
    rack.os.pacing.method = raven_tcpip::PacingMethod::Reg;
    assert_eq!(rack.rack_chg_query(&mut timers), 1);
}

#[test]
fn rack_pru_options_rejects_oob() {
    assert_eq!(RackMisc::rack_pru_options(0), Ok(()));
    assert_eq!(RackMisc::rack_pru_options(PRUS_OOB), Err(Errno::EOPNOTSUPP));
}

#[test]
fn rack_process_option_handles_the_supported_options() {
    let mut rack = rack();
    rack.rack_process_option(TCP_RACK_SET_RXT_OPTIONS, 2)
        .expect("rxt");
    assert!(rack.shape_rxt_to_pacing_min);
    assert_eq!(
        rack.rack_process_option(TCP_RACK_SET_RXT_OPTIONS, 3),
        Err(Errno::EINVAL)
    );
    assert_eq!(rack.rack_process_option(9999, 1), Err(Errno::EOPNOTSUPP));
}
