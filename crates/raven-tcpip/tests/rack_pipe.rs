// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Public `RackPipe` dispatch checks (K333).

use raven_tcpip::{
    RACK_ACKED, RACK_INITIAL_RTO, RACK_SACK_PASSED, RACK_WAS_LOST, RackHpts, RackPipe, RackSendmap,
    TF_CONGRECOVERY, TF_FASTRECOVERY, TLP_USE_TWO_TWO,
};

fn established_pipe() -> RackPipe {
    let mut pipe = RackPipe::new();
    let tp = &mut pipe.stack.tp;
    tp.t_state = 4;
    tp.snd_una = 1_000;
    tp.snd_max = 5_000;
    tp.snd_cwnd = 10_000;
    tp.snd_ssthresh = 20_000;
    tp.t_maxseg = 1_000;
    tp.t_rxtcur = 200_000;
    pipe.stack.pace_min_segs = 1_000;
    pipe.rack_rtt = 1_000;
    pipe
}

#[test]
fn the_public_pipe_accounts_and_marks_loss() {
    let mut pipe = established_pipe();
    pipe.stack.sacked = 100;
    pipe.stack.considered_lost = 200;
    pipe.stack.holes_rxt = 50;
    assert_eq!(pipe.rack_compute_pipe(), 3_750);
    assert_eq!(pipe.do_rack_compute_pipe(2_000), 2_750);
    assert_eq!(pipe.rack_grab_rtt(), 1_000);

    pipe.push_sendmap(RackSendmap::new(1_000, 2_000, 1_000));
    pipe.push_sendmap(RackSendmap::new(2_000, 3_000, 1_000));
    pipe.sendmap_mut(0).expect("first").flags = RACK_SACK_PASSED;
    pipe.sendmap_mut(1).expect("second").flags = RACK_SACK_PASSED;
    pipe.stack.considered_lost = 0;
    pipe.rack_mark_lost(0, 2_000);
    assert_ne!(pipe.sendmap()[0].flags & RACK_WAS_LOST, 0);
    assert_ne!(pipe.sendmap()[1].flags & RACK_WAS_LOST, 0);
    assert_eq!(pipe.stack.considered_lost, 2_000);

    pipe.rack_mark_nolonger_lost(0);
    assert_eq!(pipe.sendmap()[0].flags & RACK_WAS_LOST, 0);
    assert_eq!(pipe.stack.considered_lost, 1_000);
}

#[test]
fn the_public_pipe_enters_and_leaves_recovery() {
    let mut pipe = established_pipe();
    pipe.push_sendmap(RackSendmap::new(1_000, 2_000, 1_000));
    pipe.push_sendmap(RackSendmap::new(2_000, 3_000, 1_000));
    pipe.sendmap_mut(1).expect("second").flags = RACK_ACKED;
    pipe.rack_rtt = 1_000;
    assert_eq!(pipe.rack_check_recovery_mode(2_000), Some(0));
    assert_ne!(pipe.stack.tp.t_flags & TF_FASTRECOVERY, 0);
    assert_ne!(pipe.stack.tp.t_flags & TF_CONGRECOVERY, 0);
    assert_eq!(pipe.stack.tp.snd_ssthresh, 5_000);
    assert_eq!(pipe.stack.prr_sndcnt, 1_000);
    assert_eq!(pipe.state.snd_recover, 5_000);

    pipe.rack_post_recovery(2_000);
    assert_eq!(
        pipe.stack.tp.t_flags & (TF_FASTRECOVERY | TF_CONGRECOVERY),
        0
    );
    assert_eq!(pipe.state.snd_recover, 1_000);

    pipe.stack.tp.t_flags = TF_FASTRECOVERY | TF_CONGRECOVERY;
    pipe.rack_exit_recovery(4);
    assert_eq!(
        pipe.stack.tp.t_flags & (TF_FASTRECOVERY | TF_CONGRECOVERY),
        0
    );
}

#[test]
fn the_public_pipe_thresholds_and_k313_clock() {
    let mut pipe = established_pipe();
    pipe.state.tlp_threshold_use = TLP_USE_TWO_TWO;
    pipe.state.tlp_threshold = 2;
    pipe.push_sendmap(RackSendmap::new(1_000, 2_000, 1_000));
    assert_eq!(pipe.rack_calc_thresh_tlp(0, 50_000), 115_000);
    pipe.state.pkt_delay = 200;
    assert_eq!(pipe.rack_calc_thresh_rack(1_000, 1_000), 1_200);
    assert_eq!(pipe.rack_grab_rtt(), 1_000);
    pipe.rack_rtt = 0;
    pipe.stack.tp.t_srtt = 0;
    assert_eq!(pipe.rack_grab_rtt(), RACK_INITIAL_RTO);

    pipe.hpts = RackHpts::new(10_000);
    assert_eq!(pipe.now_us(), 10_000);
    let mut clocked = established_pipe();
    clocked.hpts = RackHpts::new(2_000);
    clocked.push_sendmap(RackSendmap::new(1_000, 2_000, 1_000));
    assert_eq!(clocked.rack_check_recovery_mode_now(), Some(0));
}
