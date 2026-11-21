// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Public-surface checks for the K332 RACK state handlers.

use raven_tcpip::{
    IPPROTO_TCP, RackSegment, RackSegmentResult, RackSockopt, RackStack, RackVtable, SockoptDir,
    TCP_RACK_PACE_MAX_SEG, TCPS_CLOSED, TCPS_ESTABLISHED, TCPS_SYN_SENT, TH_ACK, TH_SYN,
    rack_do_segment, rack_do_segment_nounlock, rack_do_syn_sent, rack_handle_delayed_ack,
    rack_process_option,
};

fn running(state: u8) -> RackStack {
    let mut stack = RackStack::new();
    stack.tfb_tcp_fb_init().expect("init");
    stack.tp.t_state = state;
    stack.tp.snd_una = 100;
    stack.tp.snd_nxt = 200;
    stack.tp.snd_max = 200;
    stack.snd_wnd = 65_535;
    stack
}

fn segment(flags: u16, ack: u32, payload_len: u32) -> RackSegment {
    RackSegment {
        seq: 100,
        ack,
        flags,
        payload_len,
        window: 4_096,
        ecn: 0,
    }
}

#[test]
fn public_syn_sent_handler_establishes_the_connection() {
    let mut stack = running(TCPS_SYN_SENT);
    stack.tp.snd_una = 0;
    stack.tp.snd_max = 100;
    let result = rack_do_syn_sent(&mut stack, &segment(TH_SYN | TH_ACK, 100, 0));
    assert_eq!(result, RackSegmentResult::Acked(100));
    assert_eq!(stack.tp.t_state, TCPS_ESTABLISHED);
    assert_eq!(stack.snd_wnd, 4_096);
}

#[test]
fn public_segment_dispatch_processes_data_and_ignores_closed() {
    let mut established = running(TCPS_ESTABLISHED);
    assert_eq!(
        rack_do_segment(&mut established, &segment(0, 100, 8)),
        RackSegmentResult::Data
    );

    let mut closed = running(TCPS_CLOSED);
    assert_eq!(
        rack_do_segment_nounlock(&mut closed, &segment(TH_ACK, 100, 8)),
        RackSegmentResult::Ignored
    );
}

#[test]
fn public_option_and_delayed_ack_paths_work() {
    let mut stack = running(TCPS_ESTABLISHED);
    let get = RackSockopt {
        dir: SockoptDir::Get,
        level: IPPROTO_TCP,
        name: TCP_RACK_PACE_MAX_SEG,
        value: 0,
    };
    assert!(
        rack_process_option(&mut stack, &get)
            .expect("get")
            .is_some()
    );
    assert!(rack_handle_delayed_ack(&mut stack));
    assert!(stack.timer_exp > 0);
    assert!(!rack_handle_delayed_ack(&mut stack));
}
