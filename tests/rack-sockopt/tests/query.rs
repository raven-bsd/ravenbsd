// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Every K334 query request: SENDMAP, TIMERS_UP, RACK_TIMES and invalid.

use raven_tcpip::{
    MiscQuery, PacingMethod, RackMisc, TCP_QUERY_RACK_TIMES, TCP_QUERY_SENDMAP, TCP_QUERY_TIMERS_UP,
};

#[test]
fn sendmap_query_returns_the_matching_map() {
    let mut rack = RackMisc::new();
    rack.tp.snd_una = 100;
    rack.tp.snd_nxt = 300;
    rack.tp.snd_max = 300;
    let id = rack.rack_alloc().expect("map");
    if let Some(map) = rack.sendmaps.get_mut(&id) {
        map.start = 100;
        map.end = 180;
        map.rtr_cnt = 2;
        map.dupack = 3;
        map.fas = 12;
        map.flags = raven_tcpip::RACK_HAS_FIN;
        map.rtr_bytes = 44;
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
    assert_eq!(query.sendmap_fas, 12);
    assert_eq!(query.sendmap_flags, raven_tcpip::RACK_HAS_FIN);
    assert_eq!(query.sendmap_r_rtr_bytes, 44);
    assert_eq!(query.sendmap_time[0], 9);
}

#[test]
fn sendmap_query_returns_zero_for_unknown_or_empty_flight() {
    let mut rack = RackMisc::new();
    rack.tp.snd_una = 100;
    rack.tp.snd_max = 300;
    let mut query = MiscQuery {
        req: TCP_QUERY_SENDMAP,
        req_param: 120,
        ..MiscQuery::default()
    };
    assert_eq!(rack.rack_chg_query(&mut query), 0);
    rack.tp.snd_max = rack.tp.snd_una;
    assert_eq!(rack.rack_chg_query(&mut query), 0);
}

#[test]
fn timers_up_query_tracks_the_pacing_method() {
    let mut rack = RackMisc::new();
    let mut query = MiscQuery {
        req: TCP_QUERY_TIMERS_UP,
        ..MiscQuery::default()
    };
    assert_eq!(rack.rack_chg_query(&mut query), 0);
    rack.os.pacing.method = PacingMethod::Reg;
    rack.os.now_us = 123_456;
    assert_eq!(rack.rack_chg_query(&mut query), 1);
    assert_eq!(query.timer_hpts_flags, 1);
    assert_eq!(query.timer_pacing_to, 123_456);
    assert_eq!(query.timer_timer_exp, 123_456);
}

#[test]
fn rack_times_query_reports_the_k334_state() {
    let mut rack = RackMisc::new();
    rack.tp.t_srtt = 77;
    let mut query = MiscQuery {
        req: TCP_QUERY_RACK_TIMES,
        ..MiscQuery::default()
    };
    assert_eq!(rack.rack_chg_query(&mut query), 1);
    assert_eq!(query.rack_rtt, 77);
    assert_eq!(query.rack_srtt_measured, 1);
    assert_eq!(query.rack_num_dsacks, 0);
}

#[test]
fn invalid_query_returns_einval() {
    let rack = RackMisc::new();
    let mut query = MiscQuery {
        req: 999,
        ..MiscQuery::default()
    };
    assert_eq!(rack.rack_chg_query(&mut query), -22);
}
