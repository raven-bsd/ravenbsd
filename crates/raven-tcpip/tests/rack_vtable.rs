// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Public `RackVtable` dispatch checks (K318).

use raven_tcpip::{
    Errno, IPPROTO_TCP, PACE_TMR_RXT, RackOptions, RackPortState, RackQuery, RackSegment,
    RackSegmentResult, RackSockopt, RackStack, RackVtable, SockoptDir, TCP_QUERY_RACK_TIMES,
    TCP_QUERY_TIMERS_UP, TCP_RACK_PACE_MAX_SEG, TCP_RACK_PACING_DIVISOR, TCPS_ESTABLISHED, TH_ACK,
};

fn established_stack() -> RackStack {
    let mut stack = RackStack::new();
    stack.tfb_tcp_fb_init().expect("init");
    stack.tp.t_state = TCPS_ESTABLISHED;
    stack.tp.snd_una = 1_000;
    stack.tp.snd_nxt = 1_000;
    stack.tp.snd_max = 1_000;
    stack.tp.snd_cwnd = 16 * 1_460;
    stack.tp.t_maxseg = 1_460;
    stack
}

fn enqueue(stack: &mut RackStack, len: usize) {
    let mbuf = stack.os.m_getcl(0).expect("mbuf");
    let data = vec![0x33; len];
    assert!(stack.os.mbufs.write(mbuf, 0, &data));
    assert!(stack.os.mbufs.set_len(mbuf, len));
    stack.os.sbappendstream_locked(Some(mbuf)).expect("append");
}

#[test]
fn the_public_vtable_runs_output_ack_and_queries() {
    let mut stack = established_stack();
    enqueue(&mut stack, 2_000);
    assert_eq!(stack.tfb_tcp_output(), Ok(()));
    assert_eq!(stack.tp.snd_max, 2_460);
    assert_eq!(stack.os.sbused(), 2_000);

    let segment = RackSegment {
        seq: 1_000,
        ack: 2_460,
        flags: TH_ACK,
        payload_len: 0,
        window: 32_000,
        ecn: 0,
    };
    assert_eq!(
        stack.tfb_do_segment_nounlock(&segment),
        Ok(RackSegmentResult::Acked(1_460))
    );
    assert_eq!(stack.tp.snd_una, 2_460);
    assert_eq!(stack.os.sbused(), 540);
    assert_eq!(stack.snd_wnd, 32_000);
    assert!(stack.wanted_output);

    let mut query = RackQuery {
        req: TCP_QUERY_TIMERS_UP,
        ..RackQuery::default()
    };
    assert_eq!(stack.tfb_chg_query(&mut query), 0);
    let deadline = stack.os.tcp_hpts_insert(50);
    stack.hpts_flags = u32::from(PACE_TMR_RXT);
    stack.timer_exp = deadline;
    assert_eq!(stack.tfb_chg_query(&mut query), 1);
    assert_eq!(query.timer_hpts_flags, u32::from(PACE_TMR_RXT));

    let mut times = RackQuery {
        req: TCP_QUERY_RACK_TIMES,
        ..RackQuery::default()
    };
    stack.rack_min_rtt = 77;
    assert_eq!(stack.tfb_chg_query(&mut times), 1);
    assert_eq!(times.rack_min_rtt, 77);

    let info = stack.tfb_stack_info();
    assert_eq!(info.bytes_transmitted, 1_460);
    assert_eq!(stack.tfb_compute_pipe(), 0);
}

#[test]
fn the_public_vtable_handles_options_state_and_inherit() {
    let mut stack = established_stack();
    let set = RackSockopt {
        dir: SockoptDir::Set,
        level: IPPROTO_TCP,
        name: TCP_RACK_PACE_MAX_SEG,
        value: 8,
    };
    assert_eq!(stack.tfb_tcp_ctloutput(&set), Ok(None));
    let get = RackSockopt {
        dir: SockoptDir::Get,
        ..set
    };
    assert_eq!(stack.tfb_tcp_ctloutput(&get), Ok(Some(8)));

    let set = RackSockopt {
        dir: SockoptDir::Set,
        level: IPPROTO_TCP,
        name: TCP_RACK_PACING_DIVISOR,
        value: 500,
    };
    assert_eq!(stack.tfb_tcp_ctloutput(&set), Ok(None));
    assert_eq!(stack.os.pacing.divisor, 500);

    assert_eq!(stack.tfb_tcp_handoff_ok(), Err(Errno::EINVAL));
    stack.tp.t_flags |= raven_tcpip::TF_SACK_PERMIT;
    assert_eq!(stack.tfb_tcp_handoff_ok(), Ok(()));
    stack.tp.t_flags |= raven_tcpip::TF_SENTFIN;
    stack.tp.snd_max = stack.tp.snd_una + 10;
    assert_eq!(stack.tfb_tcp_handoff_ok(), Err(Errno::EAGAIN));

    stack.tfb_hwtls_change(true);
    assert!(stack.hw_tls);
    assert_eq!(
        stack.tfb_pru_options(raven_tcpip::PRUS_OOB),
        Err(Errno::EOPNOTSUPP)
    );

    stack.tp.t_maxseg = 1_200;
    stack.tfb_tcp_mtu_chg();
    assert!(stack.must_retran);

    let mut parent = established_stack();
    parent.options = RackOptions {
        mbuf_queue: true,
        use_cmp_ack: true,
        ..RackOptions::default()
    };
    stack.tfb_inherit(&parent);
    assert_eq!(stack.options, parent.options);
    assert_ne!(stack.t_flags2 & raven_tcpip::TF2_SUPPORTS_MBUFQ, 0);

    stack.tfb_tcp_fb_fini(true);
    assert_eq!(stack.state, RackPortState::Fini);
    assert!(stack.tcb_is_purged);
}
