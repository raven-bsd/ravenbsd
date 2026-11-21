// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Unit tests for the FreeBSD `tcp_output` send gate and `tcp_setpersist` backoff.

use super::*;
use crate::tcp_state::TcpConnection;

fn connection() -> TcpConnection {
    let mut connection = TcpConnection::new_established_for_test(Instant::ZERO);
    connection.snd_una = 1_000;
    connection.snd_nxt = 1_000;
    connection.snd_max = None;
    connection.snd_wnd = 65_535;
    connection.max_snd_wnd = 65_535;
    connection.mss = 1_000;
    connection.nodelay = false;
    connection
}

#[test]
fn the_send_window_is_the_minimum_of_peer_and_congestion_window() {
    let mut connection = connection();
    connection.snd_wnd = 4_000;
    let plan = freebsd_output_plan(&mut connection, 8_000, 2_000, false);
    assert_eq!(plan.room, 2_000);
    assert_eq!(plan.len, 1_000, "one MSS");
    assert!(plan.send);
}

#[test]
fn a_segment_at_the_mss_is_sent() {
    let mut connection = connection();
    let plan = freebsd_output_plan(&mut connection, 1_000, 10_000, false);
    assert_eq!(plan.len, 1_000);
    assert!(plan.send);
}

#[test]
fn a_closed_window_with_queued_data_arms_persist() {
    let mut connection = connection();
    connection.snd_wnd = 0;
    let plan = freebsd_output_plan(&mut connection, 44, 10_000, false);
    assert_eq!(plan.len, 0);
    assert!(!plan.send);
    assert!(plan.arm_persist);
    assert!(connection.zero_window_deadline.is_none());
}

#[test]
fn a_forced_closed_window_sends_exactly_one_byte() {
    let mut connection = connection();
    connection.snd_wnd = 0;
    let plan = freebsd_output_plan(&mut connection, 44, 10_000, true);
    assert_eq!(plan.room, 1, "TF_FORCEDATA uses a one-byte window");
    assert_eq!(plan.len, 1);
    assert!(plan.send);
    assert!(!plan.arm_persist);
}

#[test]
fn a_forced_nonzero_window_leaves_persist_state() {
    let mut connection = connection();
    connection.zero_window_deadline = Some(Instant::from_millis(9_000));
    connection.zero_window_backoff_ms = 4;
    let _ = freebsd_output_plan(&mut connection, 44, 10_000, true);
    assert!(connection.zero_window_deadline.is_none());
    assert_eq!(connection.zero_window_backoff_ms, 0);
}

#[test]
fn nagle_holds_a_small_write_behind_unacked_data() {
    let mut connection = connection();
    connection.snd_nxt = 1_100;
    connection.snd_max = Some(1_100);
    let plan = freebsd_output_plan(&mut connection, 144, 10_000, false);
    assert_eq!(plan.off, 100);
    assert_eq!(plan.len, 44);
    assert!(!plan.send, "44 bytes with 100 in flight is a Nagle hold");
    assert!(plan.arm_persist, "the queued tail still needs a timer");
}

#[test]
fn nodelay_releases_a_small_tail() {
    let mut connection = connection();
    connection.snd_nxt = 1_100;
    connection.snd_max = Some(1_100);
    connection.nodelay = true;
    let plan = freebsd_output_plan(&mut connection, 144, 10_000, false);
    assert!(plan.send);
}

#[test]
fn an_idle_connection_sends_a_small_tail() {
    let mut connection = connection();
    let plan = freebsd_output_plan(&mut connection, 44, 10_000, false);
    assert_eq!(plan.off, 0);
    assert!(plan.send, "idle plus all data fits");
}

#[test]
fn a_retransmission_below_the_high_water_is_sent() {
    let mut connection = connection();
    connection.snd_nxt = 1_100;
    connection.snd_max = Some(3_000);
    let plan = freebsd_output_plan(&mut connection, 500, 10_000, false);
    assert_eq!(
        plan.off, 100,
        "the payload always starts at snd_nxt, not the SACK pipe"
    );
    assert_eq!(plan.len, 400, "the unsent tail");
    assert!(plan.send, "snd_nxt < snd_max opens the gate");
}

#[test]
fn half_the_maximum_window_opens_the_gate() {
    let mut connection = connection();
    connection.snd_nxt = 1_100;
    connection.snd_max = Some(1_100);
    connection.max_snd_wnd = 1_800;
    let plan = freebsd_output_plan(&mut connection, 1_000, 10_000, false);
    assert_eq!(plan.len, 900);
    assert!(plan.send, "900 >= max_sndwnd / 2");
}

#[test]
fn a_zero_maximum_window_does_not_open_the_gate() {
    let mut connection = connection();
    connection.snd_nxt = 1_100;
    connection.snd_max = Some(1_100);
    connection.max_snd_wnd = 0;
    let plan = freebsd_output_plan(&mut connection, 1_000, 10_000, false);
    assert_eq!(plan.len, 900);
    assert!(
        !plan.send,
        "FreeBSD guards the half-window rule with max_sndwnd > 0"
    );
}

#[test]
fn a_shrunken_closed_window_rewinds_snd_nxt_and_arms_persist() {
    let mut connection = connection();
    connection.snd_nxt = 2_000;
    connection.snd_max = Some(2_000);
    connection.snd_wnd = 0;
    connection.rto_deadline = Some(Instant::from_millis(5_000));
    let plan = freebsd_output_plan(&mut connection, 2_000, 10_000, false);
    assert_eq!(plan.len, 0);
    assert_eq!(
        connection.snd_nxt, 1_000,
        "snd_nxt returns to the window edge"
    );
    assert!(connection.rto_deadline.is_none());
    assert!(plan.arm_persist);
}

#[test]
fn a_stale_retransmit_timer_does_not_block_persist() {
    let mut connection = connection();
    connection.snd_wnd = 0;
    connection.rto_deadline = Some(Instant::from_millis(9_000));
    let plan = freebsd_output_plan(&mut connection, 44, 10_000, false);
    assert!(connection.rto_deadline.is_none());
    assert!(plan.arm_persist);
}

#[test]
fn persist_shift_normalizes_the_existing_field() {
    assert_eq!(persist_shift(0), 0);
    assert_eq!(persist_shift(7), 7);
    assert_eq!(persist_shift(12), 12);
    assert_eq!(persist_shift(1_000), 0);
}

#[test]
fn persist_interval_uses_the_freebsd_backoff_and_60_second_ceiling() {
    assert_eq!(persist_interval_ms(2_000, 0), 1_000);
    assert_eq!(persist_interval_ms(2_000, 1), 2_000);
    assert_eq!(persist_interval_ms(2_000, 2), 4_000);
    assert_eq!(persist_interval_ms(2_000, 3), 8_000);
    assert_eq!(persist_interval_ms(2_000, 4), 16_000);
    assert_eq!(persist_interval_ms(2_000, 5), 32_000);
    assert_eq!(persist_interval_ms(2_000, 6), 60_000);
    assert_eq!(persist_interval_ms(2_000, 12), 60_000);
}

#[test]
fn the_persist_backoff_table_is_the_freebsd_table() {
    assert_eq!(
        TCP_BACKOFF,
        [1, 2, 4, 8, 16, 32, 64, 128, 256, 512, 512, 512, 512]
    );
    assert_eq!(TCP_MAXRXTSHIFT, 12);
    assert_eq!(TCP_TOTBACKOFF, 2_559);
}
