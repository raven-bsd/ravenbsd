// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Unit tests for every K332 RACK state/handler function.

use super::*;
use crate::ecn::{TH_ACK, TH_SYN};
use crate::rack_vtable::{SockoptDir, TCP_PACING_DND, TCP_RACK_PACE_MAX_SEG, TH_FIN, TH_RST};

fn running(state: u8) -> RackStack {
    let mut stack = RackStack::new();
    stack.tfb_tcp_fb_init().expect("init");
    stack.tp.t_state = state;
    stack.tp.snd_una = 100;
    stack.tp.snd_nxt = 200;
    stack.tp.snd_max = 200;
    stack.tp.snd_cwnd = 10 * 1_460;
    stack.tp.t_maxseg = 1_460;
    stack.snd_wnd = 65_535;
    stack
}

fn segment(flags: u16, ack: u32, payload_len: u32, window: u16) -> RackSegment {
    RackSegment {
        seq: 100,
        ack,
        flags,
        payload_len,
        window,
        ecn: 0,
    }
}

#[test]
fn rack_do_syn_sent_establishes_on_syn_ack() {
    let mut stack = running(TCPS_SYN_SENT);
    stack.tp.snd_una = 0;
    stack.tp.snd_max = 100;
    let result = rack_do_syn_sent(&mut stack, &segment(TH_SYN | TH_ACK, 100, 0, 4_096));
    assert_eq!(result, RackSegmentResult::Acked(100));
    assert_eq!(stack.tp.t_state, TCPS_ESTABLISHED);
    assert_eq!(stack.tp.snd_una, 100);
    assert_eq!(stack.snd_wnd, 4_096);
}

#[test]
fn rack_do_syn_sent_closes_on_rst() {
    let mut stack = running(TCPS_SYN_SENT);
    let result = rack_do_syn_sent(&mut stack, &segment(TH_RST, 0, 0, 0));
    assert_eq!(result, RackSegmentResult::Reset);
    assert_eq!(stack.tp.t_state, TCPS_CLOSED);
}

#[test]
fn rack_do_syn_recv_establishes_on_ack() {
    let mut stack = running(TCPS_SYN_RECEIVED);
    stack.tp.snd_una = 0;
    stack.tp.snd_max = 100;
    let result = rack_do_syn_recv(&mut stack, &segment(TH_ACK, 100, 0, 8_192));
    assert_eq!(result, RackSegmentResult::Acked(100));
    assert_eq!(stack.tp.t_state, TCPS_ESTABLISHED);
    assert_eq!(stack.tp.snd_una, 100);
    assert_eq!(stack.snd_wnd, 8_192);
}

#[test]
fn rack_do_established_clears_the_rto_override() {
    let mut stack = running(TCPS_ESTABLISHED);
    stack.snd_max_at_rto = 100;
    stack.must_retran = true;
    stack.timer_override = true;
    let result = rack_do_established(&mut stack, &segment(TH_ACK, 200, 0, 65_535));
    assert!(matches!(result, RackSegmentResult::Acked(_)));
    assert_eq!(stack.tp.snd_una, 200);
    assert!(!stack.must_retran);
    assert!(!stack.timer_override);
    assert_eq!(stack.snd_max_at_rto, 0);
}

#[test]
fn rack_do_fastnewdata_requires_established() {
    let mut stack = running(TCPS_SYN_SENT);
    let result = rack_do_fastnewdata(&mut stack, &segment(TH_ACK, 200, 10, 65_535));
    assert_eq!(result, RackSegmentResult::Ignored);
}

#[test]
fn rack_do_close_wait_moves_to_last_ack_on_fin() {
    let mut stack = running(TCPS_CLOSE_WAIT);
    let result = rack_do_close_wait(&mut stack, &segment(TH_FIN | TH_ACK, 100, 0, 65_535));
    assert_eq!(result, RackSegmentResult::Fin);
    assert_eq!(stack.tp.t_state, TCPS_LAST_ACK);
}

#[test]
fn rack_do_fin_wait_1_moves_to_fin_wait_2_on_ack() {
    let mut stack = running(TCPS_FIN_WAIT_1);
    stack.tp.snd_max = 200;
    let result = rack_do_fin_wait_1(&mut stack, &segment(TH_ACK, 200, 0, 65_535));
    assert!(matches!(result, RackSegmentResult::Acked(_)));
    assert_eq!(stack.tp.t_state, TCPS_FIN_WAIT_2);
}

#[test]
fn rack_do_fin_wait_2_moves_to_time_wait_on_fin() {
    let mut stack = running(TCPS_FIN_WAIT_2);
    let result = rack_do_fin_wait_2(&mut stack, &segment(TH_FIN | TH_ACK, 100, 0, 65_535));
    assert_eq!(result, RackSegmentResult::Fin);
    assert_eq!(stack.tp.t_state, TCPS_TIME_WAIT);
}

#[test]
fn rack_do_closing_moves_to_time_wait_on_ack() {
    let mut stack = running(TCPS_CLOSING);
    stack.tp.snd_max = 200;
    let result = rack_do_closing(&mut stack, &segment(TH_ACK, 200, 0, 65_535));
    assert!(matches!(result, RackSegmentResult::Acked(_)));
    assert_eq!(stack.tp.t_state, TCPS_TIME_WAIT);
}

#[test]
fn rack_do_lastack_closes_on_ack() {
    let mut stack = running(TCPS_LAST_ACK);
    stack.tp.snd_max = 200;
    let result = rack_do_lastack(&mut stack, &segment(TH_ACK, 200, 0, 65_535));
    assert!(matches!(result, RackSegmentResult::Acked(_)));
    assert_eq!(stack.tp.t_state, TCPS_CLOSED);
}

#[test]
fn rack_do_segment_dispatches_by_state() {
    let mut stack = running(TCPS_ESTABLISHED);
    let result = rack_do_segment(&mut stack, &segment(0, 100, 8, 65_535));
    assert_eq!(result, RackSegmentResult::Data);
    assert_ne!(stack.hpts_flags & PACE_TMR_DELACK, 0);
}

#[test]
fn rack_do_segment_nounlock_ignores_closed() {
    let mut stack = running(TCPS_CLOSED);
    let result = rack_do_segment_nounlock(&mut stack, &segment(TH_ACK, 100, 8, 65_535));
    assert_eq!(result, RackSegmentResult::Ignored);
    assert_eq!(stack.tp.t_state, TCPS_CLOSED);
}

#[test]
fn rack_process_data_arms_delayed_ack() {
    let mut stack = running(TCPS_ESTABLISHED);
    stack.os.advance_us(1_000);
    let result = rack_process_data(&mut stack, &segment(0, 100, 8, 65_535));
    assert_eq!(result, RackSegmentResult::Data);
    assert_ne!(stack.hpts_flags & PACE_TMR_DELACK, 0);
    assert_ne!(stack.tp.t_acktime, 0);
    assert!(stack.timer_exp > stack.os.now_us);
}

#[test]
fn rack_process_data_returns_fin_for_duplicate_ack_fin() {
    let mut stack = running(TCPS_ESTABLISHED);
    let result = rack_process_data(&mut stack, &segment(TH_FIN | TH_ACK, 100, 0, 65_535));
    assert_eq!(result, RackSegmentResult::Fin);
}

#[test]
fn rack_process_option_routes_the_option() {
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

    let set = RackSockopt {
        dir: SockoptDir::Set,
        level: IPPROTO_TCP,
        name: TCP_PACING_DND,
        value: 1,
    };
    assert_eq!(rack_process_option(&mut stack, &set), Ok(None));
    assert!(stack.pacing_dnd);
}

#[test]
fn rack_handle_delayed_ack_arms_only_once() {
    let mut stack = running(TCPS_ESTABLISHED);
    assert!(rack_handle_delayed_ack(&mut stack));
    let deadline = stack.timer_exp;
    assert_ne!(stack.hpts_flags & PACE_TMR_DELACK, 0);
    assert!(!rack_handle_delayed_ack(&mut stack));
    assert_eq!(stack.timer_exp, deadline);
}

#[test]
fn rack_handle_might_revert_clears_the_override() {
    let mut stack = running(TCPS_ESTABLISHED);
    stack.snd_max_at_rto = 100;
    stack.must_retran = true;
    stack.timer_override = true;
    assert!(rack_handle_might_revert(&mut stack));
    assert!(!stack.must_retran);
    assert!(!stack.timer_override);
    assert!(!rack_handle_might_revert(&mut stack));
}

#[test]
fn rack_handle_probe_response_opens_the_window() {
    let mut stack = running(TCPS_ESTABLISHED);
    stack.in_persist = true;
    stack.hpts_flags |= PACE_TMR_PERSIT;
    let _ = stack.os.tcp_hpts_insert(1_000);
    assert!(rack_handle_probe_response(&mut stack, 1_024));
    assert_eq!(stack.snd_wnd, 1_024);
    assert!(!stack.in_persist);
    assert_eq!(stack.hpts_flags & PACE_TMR_PERSIT, 0);
}

#[test]
fn rack_do_win_updates_changes_the_window_once() {
    let mut stack = running(TCPS_ESTABLISHED);
    stack.snd_wnd = 0;
    stack.in_persist = true;
    stack.hpts_flags |= PACE_TMR_PERSIT;
    let update = segment(TH_ACK, 100, 0, 1_024);
    assert!(rack_do_win_updates(&mut stack, &update));
    assert_eq!(stack.snd_wnd, 1_024);
    assert!(!stack.in_persist);
    assert!(!rack_do_win_updates(&mut stack, &update));
}

#[test]
fn rack_do_compressed_ack_processing_drains_the_queue() {
    let mut stack = running(TCPS_ESTABLISHED);
    stack.enqueue(segment(TH_ACK, 100, 0, 65_535));
    assert!(!rack_do_compressed_ack_processing(&mut stack, true));
    assert!(stack.queue.is_empty());
}
