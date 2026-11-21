// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Independent matrix over the merged RACK misc and vtable public paths.

use raven_tcpip::{
    IPPROTO_TCP, MiscQuery, PRUS_OOB, RACK_DEFAULT_PACING_DIVISOR, RACK_HAS_SYN, RackFunctionBlock,
    RackMisc, RackQuery, RackSegment, RackSendmap, RackSockopt, RackStack, RackVtable, SockoptDir,
    TCP_INFO, TCP_QUERY_RACK_TIMES, TCP_QUERY_SENDMAP, TCP_RACK_PACE_MIN_SEG, TCP_RACK_PROFILE,
    TCP_RACK_TIMER_SLOP, TCPS_CLOSED, TCPS_ESTABLISHED, TH_ACK,
};

fn segment(ack: u32) -> RackSegment {
    RackSegment {
        seq: 0,
        ack,
        flags: TH_ACK,
        payload_len: 0,
        window: 65_535,
        ecn: 0,
    }
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
fn misc_sendmap_and_lifecycle_matrix() {
    let mut misc = RackMisc::new();
    let first = misc.rack_alloc().expect("first map");
    let second = misc.rack_alloc_full_limit().expect("second map");
    let third = misc.rack_alloc_limit(1).expect("split map");
    assert!(misc.rack_free(first));
    misc.rack_free_trim();
    assert!(!misc.rack_free(9_999), "unknown map is not freed");

    if let Some(map) = misc.sendmaps.get_mut(&second) {
        map.start = 100;
        map.end = 200;
        map.flags = RACK_HAS_SYN;
    }
    let clone = misc.rack_clone_rsm(second, 150).expect("clone");
    assert_eq!(misc.sendmaps.get(&clone).map(|map| map.start), Some(150));
    assert!(misc.rack_merge_rsm(second, clone));

    misc.rack_init_fsb_block();
    assert_eq!(misc.fsb.header_len, 40);
    assert!(misc.rack_init_fsb().is_ok());
    misc.tp.t_state = TCPS_ESTABLISHED;
    misc.tp.snd_cwnd = 10 * 1_460;
    misc.rack_deferred_init();
    assert!(misc.deferred_inited);
    misc.rack_init_retransmit_value(2);
    assert!(misc.shape_rxt_to_pacing_min);
    misc.tp.snd_una = 0;
    misc.tp.snd_max = 0;
    misc.iss = 0;
    let outstanding = misc.rack_init_outstanding().expect("outstanding");
    assert_ne!(misc.sendmaps.get(&outstanding).map(|map| map.flags), None);
    assert!(misc.rack_init().is_ok());
    assert!(misc.initialized);
    misc.rack_mtu_change();
    misc.rack_sndbuf_autoscale();
    let freed = misc.rack_fini();
    assert!(freed > 0);
    assert!(!misc.initialized);
    assert_eq!(third, third);
}

#[test]
fn misc_options_query_and_trace_matrix() {
    let mut misc = RackMisc::new();
    misc.rack_set_profile(1).expect("profile 1");
    assert!(misc.dgp_on && misc.always_pace && misc.use_cmp_ack && misc.mbuf_queue);
    assert_eq!(misc.rack_stack_information().bytes_transmitted, 0);

    assert!(misc.rack_add_deferred_option(TCP_RACK_TIMER_SLOP, 7));
    assert_eq!(misc.rack_apply_deferred_options(), 1);
    assert_eq!(misc.timer_slop, 7);

    misc.rack_set_sockopt(&sockopt(SockoptDir::Set, TCP_RACK_PACE_MIN_SEG, 3))
        .expect("set min seg");
    assert_eq!(
        misc.rack_get_sockopt(&sockopt(SockoptDir::Get, TCP_RACK_PACE_MIN_SEG, 0)),
        Ok(Some(3))
    );
    misc.rack_process_option(TCP_RACK_PROFILE, 0)
        .expect("profile option");
    assert_eq!(misc.profile, 0);

    let mut parent = RackMisc::new();
    parent.full_size_rxt = true;
    parent.timer_slop = 9;
    assert!(misc.rack_inherit(&parent) > 0);
    assert!(misc.full_size_rxt);

    misc.tp.t_state = TCPS_ESTABLISHED;
    misc.tp.snd_max = 100;
    misc.tp.snd_una = 0;
    let info = misc.rack_fill_info();
    assert_eq!(info.state, TCPS_ESTABLISHED);
    assert_eq!(info.snd_wnd, misc.snd_wnd);
    assert_eq!(misc.rack_stack_information().bytes_retransmitted, 0);

    let id = misc.rack_alloc().expect("map");
    if let Some(map) = misc.sendmaps.get_mut(&id) {
        map.start = 0;
        map.end = 100;
        map.dupack = 2;
    }
    let mut query = MiscQuery {
        req: TCP_QUERY_SENDMAP,
        req_param: 50,
        ..MiscQuery::default()
    };
    assert_eq!(misc.rack_chg_query(&mut query), 1);
    assert_eq!(query.sendmap_start, 0);
    assert_eq!(query.sendmap_end, 100);

    let mut memory = [0x5a_u8; 16];
    assert_eq!(RackMisc::rack_ctor(&mut memory), 0);
    assert!(memory.iter().all(|byte| *byte == 0));
    RackMisc::rack_dtor(&mut memory);
    assert_eq!(
        RackMisc::rack_pru_options(PRUS_OOB),
        Err(raven_tcpip::Errno::EOPNOTSUPP)
    );
    assert_eq!(RackMisc::rack_pru_options(0), Ok(()));
    assert_eq!(
        misc.rack_get_sockopt(&sockopt(SockoptDir::Get, TCP_INFO, 0)),
        Err(raven_tcpip::Errno::EOPNOTSUPP)
    );
}

#[test]
fn vtable_matrix_calls_every_callback() {
    let mut stack = RackStack::new();
    assert_eq!(stack.tfb_tcp_fb_init(), Ok(()));
    stack.tp.t_state = TCPS_ESTABLISHED;
    stack.tp.snd_una = 100;
    stack.tp.snd_nxt = 100;
    stack.tp.snd_max = 100;
    stack.tp.snd_cwnd = 10 * 1_460;
    stack.tp.t_maxseg = 500;

    assert_eq!(stack.tfb_tcp_output(), Ok(()));
    stack.enqueue(segment(100));
    stack.tfb_tcp_do_segment(&segment(100));
    let _ = stack.tfb_do_segment_nounlock(&segment(100));
    let _ = stack.tfb_do_queued_segments(false);
    let _ = stack.tfb_tcp_ctloutput(&sockopt(SockoptDir::Set, TCP_RACK_PACE_MIN_SEG, 2));
    let _ = stack.tfb_tcp_ctloutput(&sockopt(SockoptDir::Get, TCP_RACK_PACE_MIN_SEG, 0));
    assert_eq!(stack.tfb_tcp_timer_stop_all(), Ok(()));
    stack.tfb_tcp_rexmit_tmr();
    stack.tp.t_state = TCPS_CLOSED;
    assert_eq!(stack.tfb_tcp_handoff_ok(), Ok(()));
    stack.tfb_tcp_mtu_chg();
    assert_eq!(stack.tfb_pru_options(0), Ok(()));
    stack.tfb_hwtls_change(true);
    let mut query = RackQuery {
        req: TCP_QUERY_RACK_TIMES,
        ..RackQuery::default()
    };
    assert_eq!(stack.tfb_chg_query(&mut query), 1);
    stack.tfb_switch_failed();
    let _ = stack.tfb_early_wake_check();
    stack.tp.snd_max = 1_000;
    stack.tp.snd_una = 100;
    stack.sacked = 50;
    stack.considered_lost = 25;
    stack.holes_rxt = 5;
    assert_eq!(stack.tfb_compute_pipe(), 830);
    let info = stack.tfb_stack_info();
    assert_eq!(info.bytes_transmitted, stack.bytes_transmitted);
    let mut other = RackStack::new();
    assert_eq!(other.tfb_tcp_fb_init(), Ok(()));
    stack.tfb_inherit(&other);
    stack.tfb_tcp_fb_fini(false);
}

#[test]
fn vtable_trace_values_match_the_reference_models() {
    let fb = RackFunctionBlock::rack();
    assert_eq!(fb.name, "rack");
    assert_eq!(fb.refcnt, 1);
    assert_eq!(RACK_DEFAULT_PACING_DIVISOR, 250);

    let mut stack = RackStack::new();
    assert_eq!(stack.tfb_tcp_fb_init(), Ok(()));
    stack.tp.snd_max = 1_000;
    stack.tp.snd_una = 100;
    stack.sacked = 50;
    stack.considered_lost = 25;
    stack.holes_rxt = 5;
    assert_eq!(stack.tfb_compute_pipe(), 830);

    let mut query = RackQuery {
        req: TCP_QUERY_RACK_TIMES,
        ..RackQuery::default()
    };
    stack.rack_rtt = 1234;
    stack.srtt_measured = 1;
    assert_eq!(stack.tfb_chg_query(&mut query), 1);
    assert_eq!(query.rack_rtt, 1234);
    assert_eq!(query.rack_srtt_measured, 1);

    let mut misc = RackMisc::new();
    misc.rack_set_profile(1).expect("profile");
    assert_eq!(misc.profile, 1);
    assert_eq!(misc.rack_stack_information().bytes_transmitted, 0);
    assert_eq!(
        misc.rack_get_sockopt(&sockopt(SockoptDir::Get, TCP_RACK_PACE_MIN_SEG, 0)),
        Ok(Some(0))
    );
    let _ = RackSendmap::new(10, 20, 5);
}
