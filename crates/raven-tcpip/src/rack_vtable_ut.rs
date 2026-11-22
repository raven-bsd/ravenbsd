// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Unit tests for every RACK vtable callback (K318).

use alloc::vec;

use super::*;

fn running() -> RackStack {
    let mut stack = RackStack::new();
    stack.tfb_tcp_fb_init().expect("init");
    stack.tp.t_state = TCPS_ESTABLISHED;
    stack.tp.snd_una = 100;
    stack.tp.snd_nxt = 100;
    stack.tp.snd_max = 100;
    stack.tp.snd_cwnd = 10 * 1_460;
    stack.tp.t_maxseg = 500;
    stack
}

fn enqueue_data(stack: &mut RackStack, len: usize) -> u32 {
    let mbuf = stack.os.m_getcl(0).expect("mbuf");
    let payload = vec![0x5a; len];
    assert!(stack.os.mbufs.write(mbuf, 0, &payload));
    assert!(stack.os.mbufs.set_len(mbuf, len));
    stack.os.sbappendstream_locked(Some(mbuf)).expect("append");
    u32::try_from(len).unwrap_or(u32::MAX)
}

fn ack(ack: u32) -> RackSegment {
    RackSegment {
        seq: 0,
        ack,
        flags: TH_ACK,
        payload_len: 0,
        window: 65_535,
        ecn: 0,
    }
}

#[test]
fn the_function_block_is_the_rack_block() {
    let fb = RackFunctionBlock::rack();
    assert_eq!(fb.name, "rack");
    assert_eq!(fb.id, 1);
    assert_eq!(fb.refcnt, 1);
    assert_ne!(fb.flags & TCP_FUNC_OUTPUT_CANDROP, 0);
    assert_ne!(fb.flags & TCP_FUNC_DEFAULT_OK, 0);
}

#[test]
fn fb_init_and_fini_allocate_and_release_the_uma_state() {
    let mut stack = RackStack::new();
    assert_eq!(stack.state, RackPortState::Uninitialized);
    assert_eq!(stack.tfb_tcp_fb_init(), Ok(()));
    assert_eq!(stack.state, RackPortState::Running);
    let zone = stack.zone.expect("zone");
    assert!(stack.fb_item.is_some());
    assert_eq!(stack.os.uma_zone_get_cur(zone), Some(1));
    assert_eq!(stack.pace_min_segs, stack.tp.t_maxseg);
    assert_eq!(stack.tfb_tcp_fb_init(), Err(Errno::EINVAL));

    stack.tfb_tcp_fb_fini(true);
    assert_eq!(stack.state, RackPortState::Fini);
    assert!(stack.tcb_is_purged);
    assert!(stack.fb_item.is_none());
    assert!(stack.zone.is_none());
    assert_eq!(stack.last_event, Some(0x002));

    // Fini without an allocation is a no-op, as the donor guards on `t_fb_ptr`.
    let mut empty = RackStack::new();
    empty.tfb_tcp_fb_fini(false);
    assert_eq!(empty.state, RackPortState::Uninitialized);

    // `uma_zalloc` returning NULL maps to ENOMEM.
    let mut full = RackStack::new();
    let zone = full.os.uma_zcreate("full", 1);
    assert!(full.os.uma_zone_set_max(zone, 1));
    let held = full.os.uma_zalloc(zone).expect("held");
    full.zone = Some(zone);
    assert_eq!(full.tfb_tcp_fb_init(), Err(Errno::ENOMEM));
    assert_eq!(full.state, RackPortState::Uninitialized);
    assert!(full.os.uma_zfree(zone, held));
}

#[test]
fn tcp_output_sends_without_cutting_the_send_buffer_and_arms_pacing() {
    let mut stack = running();
    let _ = enqueue_data(&mut stack, 1_000);
    stack.os.pacing.enabled = true;
    assert_eq!(stack.tfb_tcp_output(), Ok(()));
    assert_eq!(stack.tp.snd_nxt, 600);
    assert_eq!(stack.tp.snd_max, 600);
    assert_eq!(
        stack.os.sbused(),
        1_000,
        "the send buffer is cut only by ACK"
    );
    assert_eq!(stack.bytes_transmitted, 500);
    assert!(stack.os.pacing.hpts.active);
    assert_ne!(stack.hpts_flags & PACE_PKT_OUTPUT, 0);
    assert_eq!(stack.tp.t_sndtime, stack.os.tcp_ts_getticks());
    assert_eq!(stack.last_event, Some(0x100));

    assert_eq!(stack.tfb_tcp_output(), Ok(()));
    assert_eq!(stack.tp.snd_nxt, 1_100);
    assert_eq!(stack.os.sbused(), 1_000);
    assert_eq!(stack.tfb_tcp_output(), Ok(()));
    assert_eq!(stack.tp.snd_nxt, 1_100);

    let mut cold = RackStack::new();
    assert_eq!(cold.tfb_tcp_output(), Err(Errno::ENOTCONN));
}

#[test]
fn output_retransmits_after_the_rexmit_timer() {
    let mut stack = running();
    let _ = enqueue_data(&mut stack, 1_000);
    assert_eq!(stack.tfb_tcp_output(), Ok(()));
    assert_eq!(stack.tp.snd_max, 600);

    stack.tfb_tcp_rexmit_tmr();
    assert!(stack.must_retran);
    assert_eq!(stack.snd_max_at_rto, 600);
    assert_eq!(stack.tfb_tcp_output(), Ok(()));
    assert_eq!(
        stack.tp.snd_max, 600,
        "retransmission does not extend snd_max"
    );
    assert_eq!(stack.tp.snd_nxt, 600);
    assert_eq!(stack.bytes_retransmitted, 500);
    assert_eq!(stack.bytes_transmitted, 500);
    assert!(!stack.must_retran);
}

#[test]
fn do_segment_acks_bytes_and_advances_snd_una() {
    let mut stack = running();
    let _ = enqueue_data(&mut stack, 1_000);
    assert_eq!(stack.tfb_tcp_output(), Ok(()));

    assert_eq!(
        stack.tfb_do_segment_nounlock(&ack(600)),
        Ok(RackSegmentResult::Acked(500))
    );
    assert_eq!(stack.tp.snd_una, 600);
    assert_eq!(stack.os.sbused(), 500, "the ACK cut the acked bytes");
    assert_eq!(stack.tp.t_rxtshift, 0);
    assert_eq!(stack.snd_wnd, 65_535);
    assert!(stack.wanted_output);
    assert_eq!(stack.last_event, Some(0x200));

    assert_eq!(
        stack.tfb_do_segment_nounlock(&ack(600)),
        Ok(RackSegmentResult::DuplicateAck)
    );
    assert_eq!(stack.dupacks, 1);
    assert_eq!(
        stack.tfb_do_segment_nounlock(&ack(2_000)),
        Ok(RackSegmentResult::Ignored)
    );
    assert_eq!(stack.tp.snd_una, 600);
    assert_eq!(stack.os.sbused(), 500);
}

#[test]
fn do_segment_handles_fin_reset_and_data() {
    let mut stack = running();
    let fin = RackSegment {
        seq: 1,
        ack: 100,
        flags: TH_FIN,
        payload_len: 0,
        window: 0,
        ecn: 0,
    };
    assert_eq!(
        stack.tfb_do_segment_nounlock(&fin),
        Ok(RackSegmentResult::Fin)
    );
    assert_eq!(stack.tp.t_state, TCPS_CLOSE_WAIT);

    let data = RackSegment {
        flags: 0,
        payload_len: 10,
        ..fin
    };
    assert_eq!(
        stack.tfb_do_segment_nounlock(&data),
        Ok(RackSegmentResult::Data)
    );
    assert_eq!(stack.tp.t_rcvtime, stack.os.tcp_ts_getticks());

    let reset = RackSegment {
        flags: TH_RST,
        ..fin
    };
    assert_eq!(
        stack.tfb_do_segment_nounlock(&reset),
        Ok(RackSegmentResult::Reset)
    );
    assert_eq!(stack.tp.t_state, TCPS_CLOSED);
    assert!(!stack.must_retran);

    let mut cold = RackStack::new();
    assert_eq!(cold.tfb_do_segment_nounlock(&ack(1)), Err(Errno::ENOTCONN));
}

#[test]
fn do_queued_segments_drains_and_reports_a_lost_tcb() {
    let mut stack = running();
    stack.tp.snd_max = 200;
    stack.enqueue(ack(101));
    stack.enqueue(ack(102));
    assert!(!stack.tfb_do_queued_segments(true));
    assert!(stack.queue.is_empty());
    assert_eq!(stack.tp.snd_una, 102);

    let reset = RackSegment {
        seq: 0,
        ack: 0,
        flags: TH_RST,
        payload_len: 0,
        window: 0,
        ecn: 0,
    };
    stack.enqueue(reset);
    stack.enqueue(ack(200));
    assert!(stack.tfb_do_queued_segments(false));
    assert!(
        stack.queue.is_empty(),
        "the donor frees the rest of the queue"
    );
    assert_eq!(stack.tp.t_state, TCPS_CLOSED);
}

#[test]
fn do_segment_drains_the_queue_before_the_new_segment() {
    let mut stack = running();
    stack.tp.snd_max = 200;
    stack.enqueue(ack(101));
    let data = RackSegment {
        seq: 5,
        ack: 0,
        flags: 0,
        payload_len: 3,
        window: 0,
        ecn: 0,
    };
    stack.tfb_tcp_do_segment(&data);
    assert!(stack.queue.is_empty());
    assert_eq!(stack.tp.snd_una, 101);
    assert_eq!(stack.tp.t_rcvtime, stack.os.tcp_ts_getticks());
}

#[test]
fn ctloutput_sets_and_gets_the_rack_pacing_options() {
    let mut stack = running();
    let set = |name: i32, value: u32| RackSockopt {
        dir: SockoptDir::Set,
        level: IPPROTO_TCP,
        name,
        value,
    };
    let get = |name: i32| RackSockopt {
        dir: SockoptDir::Get,
        level: IPPROTO_TCP,
        name,
        value: 0,
    };

    assert_eq!(
        stack.tfb_tcp_ctloutput(&set(TCP_RACK_PACE_MAX_SEG, 1_000)),
        Ok(None)
    );
    assert_eq!(
        stack.tfb_tcp_ctloutput(&get(TCP_RACK_PACE_MAX_SEG)),
        Ok(Some(MAX_USER_SET_SEG))
    );
    assert_eq!(
        stack.tfb_tcp_ctloutput(&set(TCP_RACK_PACE_MIN_SEG, 5)),
        Ok(None)
    );
    assert_eq!(stack.options.min_segs, 5);
    assert_eq!(
        stack.tfb_tcp_ctloutput(&get(TCP_RACK_PACE_MIN_SEG)),
        Ok(Some(5))
    );
    assert_eq!(
        stack.tfb_tcp_ctloutput(&set(TCP_RACK_PACING_DIVISOR, 10)),
        Ok(None)
    );
    assert_eq!(stack.os.pacing.divisor, RL_MIN_DIVISOR);
    assert_eq!(
        stack.tfb_tcp_ctloutput(&set(TCP_RACK_PACING_DIVISOR, 0)),
        Ok(None)
    );
    assert_eq!(stack.os.pacing.divisor, RACK_DEFAULT_PACING_DIVISOR);
    assert_eq!(stack.tfb_tcp_ctloutput(&set(TCP_PACING_DND, 1)), Ok(None));
    assert!(stack.pacing_dnd);
    assert_eq!(stack.tfb_tcp_ctloutput(&get(TCP_PACING_DND)), Ok(Some(1)));

    assert_eq!(
        stack.tfb_tcp_ctloutput(&set(TCP_RACK_PACING_BETA, 60)),
        Err(Errno::EINVAL)
    );
    assert_eq!(
        stack.tfb_tcp_ctloutput(&get(TCP_RACK_PACING_BETA)),
        Ok(Some(50))
    );
    assert!(stack.os.pacing.rack_set_cc_pacing());
    stack.cc_beta = 70;
    assert_eq!(
        stack.tfb_tcp_ctloutput(&get(TCP_RACK_PACING_BETA)),
        Ok(Some(70))
    );
    stack.cc_is_newreno = false;
    assert_eq!(
        stack.tfb_tcp_ctloutput(&get(TCP_RACK_PACING_BETA)),
        Err(Errno::EINVAL)
    );
    assert_eq!(
        stack.tfb_tcp_ctloutput(&get(TCP_INFO)),
        Err(Errno::EOPNOTSUPP)
    );
    let mut ip = get(TCP_RACK_PACE_MAX_SEG);
    ip.level = 0;
    assert_eq!(stack.tfb_tcp_ctloutput(&ip), Err(Errno::EOPNOTSUPP));
}

#[test]
fn timer_stop_and_rexmit_change_real_timer_state() {
    let mut stack = running();
    stack.hpts_insert(250, PACE_TMR_RXT);
    assert!(stack.tfb_tcp_timer_stop_all().is_ok());
    assert!(stack.t_timers_stopped);
    assert!(!stack.os.pacing.hpts.active);
    assert_eq!(stack.hpts_flags, 0);

    stack.tp.snd_una = 100;
    stack.tp.snd_nxt = 400;
    stack.tp.snd_max = 400;
    stack.tp.t_rxtshift = 1;
    stack.tfb_tcp_rexmit_tmr();
    assert!(stack.timer_override);
    assert_eq!(stack.snd_max_at_rto, 400);
    assert_eq!(stack.last_timeout_snduna, 100);
    assert!(stack.must_retran);
    assert!(stack.wanted_output);
    assert_eq!(stack.last_event, Some(0x011));

    stack.tp.t_rxtshift = 3;
    stack.sacked = 42;
    stack.tfb_tcp_rexmit_tmr();
    assert!(
        !stack.must_retran,
        "the scoreboard is cleared after 3 timeouts"
    );
    assert_eq!(stack.sacked, 0);
}

#[test]
fn handoff_matches_the_donor_state_rules() {
    let mut stack = RackStack::new();
    assert_eq!(stack.tfb_tcp_handoff_ok(), Err(Errno::EINVAL));
    stack.tfb_tcp_fb_init().expect("init");

    stack.tp.t_state = TCPS_CLOSED;
    assert_eq!(stack.tfb_tcp_handoff_ok(), Ok(()));
    stack.tp.t_state = TCPS_LISTEN;
    assert_eq!(stack.tfb_tcp_handoff_ok(), Ok(()));
    stack.tp.t_state = TCPS_SYN_SENT;
    assert_eq!(stack.tfb_tcp_handoff_ok(), Err(Errno::EAGAIN));
    stack.tp.t_state = TCPS_SYN_RECEIVED;
    assert_eq!(stack.tfb_tcp_handoff_ok(), Err(Errno::EAGAIN));
    stack.tp.t_state = TCPS_ESTABLISHED;
    assert_eq!(stack.tfb_tcp_handoff_ok(), Err(Errno::EINVAL));

    stack.tp.t_flags |= TF_SACK_PERMIT;
    assert_eq!(stack.tfb_tcp_handoff_ok(), Ok(()));
    stack.tp.t_flags |= TF_SENTFIN;
    stack.tp.snd_max = 102;
    stack.tp.snd_una = 100;
    assert_eq!(stack.tfb_tcp_handoff_ok(), Err(Errno::EAGAIN));
    stack.tp.snd_max = 101;
    assert_eq!(stack.tfb_tcp_handoff_ok(), Ok(()));

    stack.tp.t_flags &= !TF_SACK_PERMIT;
    assert_eq!(stack.tfb_tcp_handoff_ok(), Err(Errno::EINVAL));
    stack.sack_not_required = true;
    assert_eq!(stack.tfb_tcp_handoff_ok(), Ok(()));
}

#[test]
fn mtu_change_retransmits_and_snaps_snd_nxt() {
    let mut stack = running();
    stack.tp.t_maxseg = 1_200;
    stack.tp.snd_nxt = 100;
    stack.tp.snd_max = 500;
    stack.tfb_tcp_mtu_chg();
    assert_eq!(stack.pace_min_segs, 1_200);
    assert_eq!(stack.tp.snd_nxt, stack.tp.snd_max);
    assert!(stack.timer_override);
    assert!(stack.must_retran);
    assert_eq!(stack.snd_max_at_rto, 500);
    assert_eq!(stack.last_event, Some(0x020));

    stack.timer_override = false;
    stack.tp.snd_nxt = 400;
    stack.tfb_tcp_mtu_chg();
    assert!(
        !stack.timer_override,
        "unchanged MSS does not re-run the timer"
    );
    assert_eq!(stack.tp.snd_nxt, 500);
}

#[test]
fn pru_options_rejects_only_oob() {
    let stack = RackStack::new();
    assert_eq!(stack.tfb_pru_options(0), Ok(()));
    assert_eq!(stack.tfb_pru_options(PRUS_OOB), Err(Errno::EOPNOTSUPP));
    assert_eq!(
        stack.tfb_pru_options(PRUS_OOB | 0x2),
        Err(Errno::EOPNOTSUPP)
    );
    assert_eq!(stack.tfb_pru_options(0x2), Ok(()));
}

#[test]
fn hwtls_change_records_the_flag() {
    let mut stack = running();
    stack.tfb_hwtls_change(true);
    assert!(stack.hw_tls);
    assert_eq!(stack.last_event, Some(0x030));
    stack.tfb_hwtls_change(false);
    assert!(!stack.hw_tls);
}

#[test]
fn chg_query_answers_timers_and_rack_times() {
    let mut stack = running();
    let mut query = RackQuery {
        req: TCP_QUERY_TIMERS_UP,
        ..RackQuery::default()
    };
    assert_eq!(stack.tfb_chg_query(&mut query), 0);

    stack.hpts_insert(40, PACE_PKT_OUTPUT | PACE_TMR_RXT);
    stack.timer_exp = stack.os.now_us + 25;
    assert_eq!(stack.tfb_chg_query(&mut query), 1);
    assert_eq!(query.timer_hpts_flags, PACE_PKT_OUTPUT | PACE_TMR_RXT);
    assert_eq!(query.timer_pacing_to, stack.last_output_to);
    assert_eq!(query.timer_timer_exp, stack.timer_exp);

    stack.num_dsacks = 2;
    stack.reorder_ts = 3;
    stack.tlp_rxt_last_time = 4;
    stack.rack_min_rtt = 5;
    stack.rack_rtt = 6;
    stack.rack_tmit_time = 7;
    stack.srtt_measured = 8;
    stack.sacked = 9;
    stack.holes_rxt = 10;
    stack.prr_delivered = 11;
    stack.prr_recovery_fs = 12;
    stack.prr_sndcnt = 13;
    stack.prr_out = 14;
    stack.tlp_out = true;
    stack.tlp_cnt_out = 15;
    stack.in_persist = true;
    stack.went_idle_time = 16;
    stack.wanted_output = true;
    let mut times = RackQuery {
        req: TCP_QUERY_RACK_TIMES,
        ..RackQuery::default()
    };
    assert_eq!(stack.tfb_chg_query(&mut times), 1);
    assert_eq!(times.rack_num_dsacks, 2);
    assert_eq!(times.rack_reorder_ts, 3);
    assert_eq!(times.rack_rxt_last_time, 4);
    assert_eq!(times.rack_min_rtt, 5);
    assert_eq!(times.rack_rtt, 6);
    assert_eq!(times.rack_tmit_time, 7);
    assert_eq!(times.rack_srtt_measured, 8);
    assert_eq!(times.rack_sacked, 9);
    assert_eq!(times.rack_holes_rxt, 10);
    assert_eq!(times.rack_prr_delivered, 11);
    assert_eq!(times.rack_prr_recovery_fs, 12);
    assert_eq!(times.rack_prr_sndcnt, 13);
    assert_eq!(times.rack_prr_out, 14);
    assert_eq!(times.rack_tlp_out, 1);
    assert_eq!(times.rack_tlp_cnt_out, 15);
    assert_eq!(times.rack_time_went_idle, 16);
    assert_eq!(times.rack_in_persist, 1);
    assert_eq!(times.rack_wanted_output, 1);

    let mut sendmap = RackQuery {
        req: TCP_QUERY_SENDMAP,
        ..RackQuery::default()
    };
    assert_eq!(stack.tfb_chg_query(&mut sendmap), 0);
    let mut unknown = RackQuery {
        req: 99,
        ..RackQuery::default()
    };
    assert_eq!(
        stack.tfb_chg_query(&mut unknown),
        i32::try_from(Errno::EINVAL.to_ret()).expect("i32")
    );
}

#[test]
fn switch_failed_sets_flags2_and_rearms_hpts() {
    let mut stack = running();
    stack.options.mbuf_queue = true;
    stack.options.use_cmp_ack = true;
    stack.os.now_us = 1_000;
    stack.tfb_switch_failed();
    assert_ne!(stack.t_flags2 & TF2_SUPPORTS_MBUFQ, 0);
    assert_ne!(stack.t_flags2 & TF2_MBUF_ACKCMP, 0);
    assert!(stack.os.pacing.hpts.active);
    assert_eq!(stack.os.pacing.hpts.deadline_us, 1_000 + 10);
    assert_eq!(stack.last_event, Some(0x040));

    stack.options.mbuf_queue = false;
    stack.options.use_cmp_ack = false;
    stack.os.pacing.enabled = false;
    stack.tp.t_state = TCPS_SYN_SENT;
    let deadline = stack.os.pacing.hpts.deadline_us;
    stack.tfb_switch_failed();
    assert_eq!(stack.t_flags2 & TF2_SUPPORTS_MBUFQ, 0);
    assert_eq!(
        stack.os.pacing.hpts.deadline_us, deadline,
        "an already queued tcb is not re-inserted"
    );
}

#[test]
fn early_wake_query_pipe_and_stack_info_read_the_state() {
    let mut stack = running();
    assert!(!stack.tfb_early_wake_check());
    stack.os.now_us = 500;
    stack.hpts_flags = PACE_TMR_RXT;
    stack.timer_exp = 500;
    assert!(stack.tfb_early_wake_check());
    stack.hpts_flags = PACE_PKT_OUTPUT;
    stack.last_output_to = 501;
    assert!(!stack.tfb_early_wake_check());
    stack.last_output_to = 500;
    assert!(stack.tfb_early_wake_check());

    stack.tp.snd_una = 100;
    stack.tp.snd_max = 500;
    stack.sacked = 40;
    stack.considered_lost = 10;
    stack.holes_rxt = 5;
    assert_eq!(stack.tfb_compute_pipe(), 355);

    let mut counters = running();
    let _ = enqueue_data(&mut counters, 500);
    assert_eq!(counters.tfb_tcp_output(), Ok(()));
    counters.tfb_tcp_rexmit_tmr();
    assert_eq!(counters.tfb_tcp_output(), Ok(()));
    let info = counters.tfb_stack_info();
    assert_eq!(info.bytes_transmitted, 500);
    assert_eq!(info.bytes_retransmitted, 500);
    assert_eq!(counters.stack_info(), info);
}

#[test]
fn inherit_copies_the_option_state_and_flags() {
    let mut source = running();
    let mut target = running();
    source.options.dgp_on = true;
    source.options.full_size_rxt = true;
    source.options.rack_tmr_std_based = true;
    source.options.saved_beta = 65;
    source.options.timer_slop = 250;
    source.options.mbuf_queue = true;
    source.options.use_cmp_ack = true;
    source.os.pacing.divisor = 300;
    target.tp.t_state = TCPS_ESTABLISHED;
    target.tfb_inherit(&source);
    assert_eq!(target.options, source.options);
    assert_eq!(target.os.pacing.divisor, 300);
    assert_ne!(target.t_flags2 & TF2_SUPPORTS_MBUFQ, 0);
    assert_ne!(target.t_flags2 & TF2_MBUF_ACKCMP, 0);
    assert_eq!(target.last_event, Some(0x050));

    // Without a `t_fb_ptr` the donor logs and returns without copying.
    let mut bare = RackStack::new();
    let untouched = bare.options.clone();
    bare.tfb_inherit(&source);
    assert_eq!(bare.options, untouched);
    assert_eq!(bare.last_event, Some(0x051));
}
