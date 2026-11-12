// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

use super::*;

fn connection() -> TcpConnection {
    let mut connection = TcpConnection::new_established_for_test(Instant::ZERO);
    connection.snd_una = 1_000;
    connection.snd_nxt = 5_000;
    connection
}

#[test]
fn highest_sent_is_snd_nxt_until_a_rewind() {
    let c = connection();
    assert_eq!(c.highest_sent(), 5_000);
    assert!(ack_window_ok(&c, 5_000));
    assert!(!ack_window_ok(&c, 5_001));
}

#[test]
fn a_rewind_keeps_acks_for_sent_data_valid() {
    let mut c = connection();
    c.rewind_for_resend();
    assert_eq!(c.snd_nxt, 1_000);
    assert_eq!(c.highest_sent(), 5_000);
    assert!(ack_window_ok(&c, 4_000), "the peer already holds this data");
    assert!(!ack_window_ok(&c, 5_001));
}

#[test]
fn sending_past_the_old_high_water_mark_moves_it() {
    let mut c = connection();
    c.rewind_for_resend();
    c.snd_nxt = 6_000;
    assert_eq!(c.highest_sent(), 6_000);
}
