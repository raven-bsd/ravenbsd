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

fn syn_segment(wscale: Option<u8>) -> Segment<'static> {
    Segment {
        seq: 100,
        ack: 0,
        flags: TcpFlags::SYN,
        ecn: 0,
        window: 65_535,
        urgent: 0,
        mss: None,
        wscale,
        tsval: None,
        tsecr: None,
        sack_permitted: false,
        sack_blocks: Vec::new(),
        payload: &[],
    }
}

#[test]
fn apply_syn_options_without_ws_does_not_negotiate() {
    let mut c = connection();
    c.offer_wscale = true;
    c.snd_wscale = 0;
    c.negotiated_wscale = None;
    apply_syn_options(&mut c, &syn_segment(None), Instant::ZERO);
    assert_eq!(c.snd_wscale, 0);
    assert_eq!(c.negotiated_wscale, None);
}

#[test]
fn apply_syn_options_with_ws_negotiates_the_peer_scale() {
    let mut c = connection();
    c.offer_wscale = true;
    apply_syn_options(&mut c, &syn_segment(Some(7)), Instant::ZERO);
    assert_eq!(c.snd_wscale, 7);
    assert_eq!(c.negotiated_wscale, Some(7));
}
