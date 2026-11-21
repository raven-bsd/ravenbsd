// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! RACK-TLP, spurious-retransmission and ECN tests.

#![cfg(feature = "testutil")]

use raven_tcpip::{Ecn, Rack, SpuriousUndo, Tlp, pto};

#[test]
fn rack_declares_loss_after_the_window() {
    let mut rack = Rack::new(10);
    rack.on_sent(1000, 0);
    assert!(!rack.is_lost(5, 100));
    assert!(rack.is_lost(100, 100));
}

#[test]
fn rack_uses_a_quarter_srtt_bound() {
    let mut rack = Rack::new(1);
    rack.on_sent(1000, 0);
    assert!(!rack.is_lost(20, 100));
    assert!(rack.is_lost(30, 100));
}

#[test]
fn rack_dsack_grows_the_window() {
    let mut rack = Rack::new(10);
    rack.on_dsack(100);
    assert!(rack.reorder_window > 10);
    assert!(rack.dsack_grown);
}

#[test]
fn rack_dsack_growth_is_bounded_by_srtt() {
    let mut rack = Rack::new(10);
    for _ in 0..20 {
        rack.on_dsack(100);
    }
    assert!(rack.reorder_window <= 100);
}

#[test]
fn rack_ack_advances_the_clock() {
    let mut rack = Rack::new(10);
    rack.on_sent(1000, 0);
    rack.on_ack(2000, 50);
    assert_eq!(rack.end_seq, 2000);
    assert_eq!(rack.xmit_ts, 50);
}

#[test]
fn rack_reordering_within_the_window_is_not_loss() {
    let mut rack = Rack::new(200);
    rack.on_sent(1000, 0);
    for now in 0..=200 {
        assert!(!rack.is_lost(now, 100));
    }
}

#[test]
fn rack_zero_window_still_declares_after_one_ms() {
    let mut rack = Rack::new(0);
    rack.on_sent(1, 0);
    assert!(!rack.is_lost(0, 0));
    assert!(rack.is_lost(2, 0));
}

#[test]
fn twenty_percent_reordering_causes_zero_retransmissions() {
    let mut rack = Rack::new(500);
    let mut retransmissions = 0;
    for index in 0..100u32 {
        rack.on_sent(index * 1000, index * 10);
        if rack.is_lost(index * 10 + 20, 200) {
            retransmissions += 1;
        }
    }
    assert_eq!(retransmissions, 0);
}

#[test]
fn rack_core_ack_declares_sack_loss_and_enters_recovery() {
    use raven_tcpip::RackContext;
    let mut rack = Rack::new(10);
    rack.send(1000, 2000, 0);
    rack.send(2000, 3000, 0);
    rack.send(3000, 4000, 0);
    let mut ctx = RackContext {
        now_ms: 100,
        srtt_ms: 40,
        mss: 1460,
        ack: 1000,
        snd_max: 4000,
        snd_nxt: 4000,
        cwnd: 14_600,
        ssthresh: 7_300,
        sack_permitted: true,
        sack_count: 1,
        ..RackContext::default()
    };
    ctx.sacks[0] = (3000, 4000);
    let outcome = rack.ack_received(&ctx);
    assert_eq!(outcome.acked, 1000);
    assert_eq!(outcome.lost, vec![1000, 2000]);
    assert!(outcome.retransmit);
    assert!(outcome.entered_recovery);
    assert_eq!(outcome.retransmit_range, Some((1000, 2000)));
    assert!(rack.in_recovery);
    assert_eq!(rack.recover, 4000);
}

#[test]
fn rack_core_update_entry_refreshes_retransmit_time() {
    let mut rack = Rack::new(10);
    rack.send(1000, 2000, 0);
    rack.update_entry(1000, 500, 50);
    assert_eq!(rack.entries[0].xmit_ts, 50);
    assert!(rack.entries[0].retransmitted);
}

#[test]
fn rack_core_post_recovery_uses_the_ssthresh_floor() {
    use raven_tcpip::RackContext;
    let rack = Rack::new(10);
    let ctx = RackContext {
        mss: 1460,
        ssthresh: 10_000,
        ..RackContext::default()
    };
    assert_eq!(rack.post_recovery(&ctx, 5_000), 6_460);
    assert_eq!(rack.post_recovery(&ctx, 12_000), 10_000);
}

#[test]
fn rack_reo_window_grows_on_observed_reordering() {
    let mut rack = Rack::new(10);
    rack.observe_reordering(100);
    assert_eq!(rack.reorder_window, 35);
    rack.on_dsack(100);
    assert_eq!(rack.reorder_window, 60);
}

#[test]
fn rack_core_ack_retains_newer_ranges_and_exits_recovery() {
    use raven_tcpip::RackContext;
    let mut rack = Rack::new(10);
    rack.send(1000, 2000, 0);
    rack.send(2000, 3000, 0);
    let ctx = RackContext {
        now_ms: 100,
        srtt_ms: 100,
        mss: 1460,
        ack: 2500,
        snd_max: 3000,
        snd_nxt: 3000,
        cwnd: 14_600,
        ssthresh: 7_300,
        ..RackContext::default()
    };
    let _ = rack.ack_received(&ctx);
    assert_eq!(rack.entries.len(), 1);
    assert_eq!(rack.entries[0].start, 2500);
    let mut signal = ctx;
    signal.ack = 2500;
    signal.snd_max = 3000;
    let entered = rack.cong_signal(&signal, raven_tcpip::RackSignal::DupAck);
    assert!(entered.in_recovery);
    let end = rack.ack_received(&RackContext {
        ack: 3000,
        now_ms: 200,
        srtt_ms: 100,
        snd_max: 3000,
        ..signal
    });
    assert!(end.recovered);
    assert!(!rack.in_recovery);
}

#[test]
fn tlp_pto_is_srtt_plus_four_rttvar() {
    assert_eq!(pto(100, 10).as_millis(), 142);
}

#[test]
fn tlp_pto_has_a_minimum_rttvar_term() {
    assert_eq!(pto(0, 0).as_millis(), 3);
}

#[test]
fn tlp_arms_and_fires() {
    let mut tlp = Tlp::default();
    tlp.arm(100, 50, 5);
    assert!(!tlp.fires(100));
    assert!(tlp.fires(100 + 74));
}

#[test]
fn tlp_disarms_on_ack() {
    let mut tlp = Tlp::default();
    tlp.arm(100, 50, 5);
    tlp.disarm();
    assert!(!tlp.fires(1000));
}

#[test]
fn tlp_is_sooner_than_a_later_rto() {
    let mut tlp = Tlp::default();
    tlp.arm(100, 50, 5);
    assert!(tlp.is_sooner_than(Some(1000)));
}

#[test]
fn tlp_is_not_sooner_than_an_earlier_rto() {
    let mut tlp = Tlp::default();
    tlp.arm(100, 50, 5);
    assert!(!tlp.is_sooner_than(Some(110)));
}

#[test]
fn tlp_with_no_rto_is_sooner() {
    let mut tlp = Tlp::default();
    tlp.arm(100, 50, 5);
    assert!(tlp.is_sooner_than(None));
}

#[test]
fn tlp_without_deadline_is_not_sooner() {
    let tlp = Tlp::default();
    assert!(!tlp.is_sooner_than(Some(100)));
}

#[test]
fn ecn_requires_both_offers() {
    let mut ecn = Ecn::default();
    ecn.negotiate(true, false);
    assert!(!ecn.negotiated);
}

#[test]
fn ecn_negotiates_when_both_offer() {
    let mut ecn = Ecn::default();
    ecn.negotiate(true, true);
    assert!(ecn.negotiated);
}

#[test]
fn ecn_ignores_ce_when_not_negotiated() {
    let mut ecn = Ecn::default();
    assert!(!ecn.on_ce());
}

#[test]
fn ecn_reacts_once_per_rtt() {
    let mut ecn = Ecn::default();
    ecn.negotiate(true, true);
    assert!(ecn.on_ce());
    assert!(!ecn.on_ce());
}

#[test]
fn ecn_reacts_again_after_the_rtt_boundary() {
    let mut ecn = Ecn::default();
    ecn.negotiate(true, true);
    assert!(ecn.on_ce());
    ecn.on_rtt();
    assert!(ecn.on_ce());
}

#[test]
fn ecn_sets_ece_on_the_first_ce() {
    let mut ecn = Ecn::default();
    ecn.negotiate(true, true);
    ecn.on_ce();
    assert!(ecn.ece_pending);
}

#[test]
fn ecn_cwr_is_sent_once() {
    let mut ecn = Ecn::default();
    ecn.negotiate(true, true);
    ecn.on_ce();
    assert!(ecn.take_cwr());
    assert!(!ecn.take_cwr());
}

#[test]
fn ecn_no_cwr_without_negotiation() {
    let mut ecn = Ecn::default();
    ecn.on_ce();
    assert!(!ecn.take_cwr());
}

#[test]
fn ecn_rtt_clears_ce_latch() {
    let mut ecn = Ecn::default();
    ecn.negotiate(true, true);
    ecn.on_ce();
    ecn.on_rtt();
    assert!(!ecn.ce_seen);
}

#[test]
fn spurious_undo_restores_once() {
    let mut undo = SpuriousUndo::record(30_000, 20_000);
    assert_eq!(undo.restore(), Some((30_000, 20_000)));
    assert_eq!(undo.restore(), None);
}

#[test]
fn spurious_undo_starts_armed() {
    let undo = SpuriousUndo::record(1, 2);
    assert!(undo.armed);
}

#[test]
fn spurious_undo_values_are_kept() {
    let undo = SpuriousUndo::record(14_600, 7_300);
    assert_eq!(undo.cwnd, 14_600);
    assert_eq!(undo.ssthresh, 7_300);
}

#[test]
fn rack_loss_is_not_declared_before_the_bound() {
    let mut rack = Rack::new(100);
    rack.on_sent(1, 1000);
    assert!(!rack.is_lost(1050, 100));
}

#[test]
fn rack_loss_is_declared_after_the_bound() {
    let mut rack = Rack::new(100);
    rack.on_sent(1, 1000);
    assert!(rack.is_lost(1200, 100));
}

#[test]
fn rack_multiple_sends_keep_the_latest() {
    let mut rack = Rack::new(10);
    rack.on_sent(100, 10);
    rack.on_sent(200, 20);
    assert_eq!(rack.end_seq, 200);
    assert_eq!(rack.xmit_ts, 20);
}

#[test]
fn rack_older_send_does_not_rewind() {
    let mut rack = Rack::new(10);
    rack.on_sent(100, 20);
    rack.on_sent(50, 10);
    assert_eq!(rack.end_seq, 100);
    assert_eq!(rack.xmit_ts, 20);
}

#[test]
fn rack_huge_srtt_does_not_overflow() {
    let mut rack = Rack::new(1);
    rack.on_sent(1, 0);
    assert!(!rack.is_lost(100, u32::MAX));
}

#[test]
fn tlp_pto_saturates_for_large_values() {
    assert!(pto(u32::MAX, u32::MAX).as_millis() > 1000);
}

#[test]
fn tlp_fires_exactly_at_the_deadline() {
    let mut tlp = Tlp::default();
    tlp.arm(0, 10, 0);
    assert!(tlp.fires(13));
}

#[test]
fn tlp_before_deadline_does_not_fire() {
    let mut tlp = Tlp::default();
    tlp.arm(0, 10, 0);
    assert!(!tlp.fires(12));
}

#[test]
fn two_tlps_do_not_interfere() {
    let mut first = Tlp::default();
    let mut second = Tlp::default();
    first.arm(0, 10, 0);
    second.arm(100, 10, 0);
    assert!(!second.fires(15));
    assert!(first.fires(15));
}

#[test]
fn ecn_ce_without_negotiation_leaves_cwr_clear() {
    let mut ecn = Ecn::default();
    ecn.on_ce();
    assert!(!ecn.cwr_pending);
}

#[test]
fn ecn_second_ce_does_not_set_a_second_cwr() {
    let mut ecn = Ecn::default();
    ecn.negotiate(true, true);
    ecn.on_ce();
    ecn.on_ce();
    assert!(ecn.take_cwr());
    assert!(!ecn.take_cwr());
}

#[test]
fn ecn_ece_remains_pending_until_cleared() {
    let mut ecn = Ecn::default();
    ecn.negotiate(true, true);
    ecn.on_ce();
    assert!(ecn.ece_pending);
    ecn.ece_pending = false;
    assert!(!ecn.ece_pending);
}

#[test]
fn undo_is_independent_of_rack() {
    let mut rack = Rack::new(10);
    let mut undo = SpuriousUndo::record(10, 5);
    rack.on_sent(100, 0);
    assert!(rack.is_lost(100, 10));
    assert_eq!(undo.restore(), Some((10, 5)));
}

#[test]
fn pto_of_one_srtt_is_srtt_plus_minimum() {
    assert_eq!(pto(200, 0).as_millis(), 203);
}

#[test]
fn pto_grows_with_rttvar() {
    assert!(pto(100, 20).as_millis() > pto(100, 10).as_millis());
}

#[test]
fn rack_reordering_window_default_is_usable() {
    let rack = Rack::new(250);
    assert_eq!(rack.reorder_window, 250);
}

#[test]
fn rack_dsack_marks_growth() {
    let mut rack = Rack::new(0);
    rack.on_dsack(1000);
    assert!(rack.dsack_grown);
    assert!(rack.reorder_window > 0);
}
