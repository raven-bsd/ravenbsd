// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

use super::*;
use crate::Instant;
use crate::pmtu::{
    IpFamily, PmtuPath, PmtuPending, PmtuResult, mss_from_peer, mss_to_advertise, on_icmp_pmtu,
};

fn connection() -> TcpConnection {
    let mut connection = TcpConnection::new_established_for_test(Instant::ZERO);
    connection.mss = 536;
    connection.peer_mss = 1460;
    connection.plp_ceiling = 1460;
    connection.snd_una = 100;
    connection.snd_nxt = 100;
    connection
}

#[test]
fn ceiling_is_bounded_by_peer_and_timestamp_overhead() {
    let mut c = connection();
    assert_eq!(ceiling(&c), 1460);
    c.timestamps = true;
    assert_eq!(ceiling(&c), 1448);
    c.peer_mss = 1000;
    assert_eq!(ceiling(&c), 988);
}

#[test]
fn an_acknowledged_probe_raises_the_segment_payload() {
    let mut c = connection();
    let mut search = Plpmtud::new(536, 1460, 8);
    search.start();
    c.plp = Some(search);
    c.plp_probe = Some((1100, 998));
    plpmtud_on_ack(&mut c, 1099, 5);
    assert_eq!(c.plp_probe, Some((1100, 998)));
    plpmtud_on_ack(&mut c, 1100, 5);
    assert_eq!(c.plp_probe, None);
    assert_eq!(c.mss, 998);
    assert_eq!(c.plp_next_ms, 5);
}

#[test]
fn a_timeout_with_a_probe_outstanding_rewinds_and_lowers_the_ceiling() {
    let mut c = connection();
    let mut search = Plpmtud::new(536, 1460, 8);
    search.start();
    c.plp = Some(search);
    c.snd_nxt = 1098;
    c.plp_probe = Some((1098, 998));
    c.rto_deadline = Some(Instant::from_millis(10));
    assert!(plpmtud_on_timeout(&mut c, 10));
    assert_eq!(c.snd_nxt, c.snd_una);
    assert_eq!(c.rto_deadline, None);
    assert_eq!(c.mss, 536);
    assert!(!plpmtud_on_timeout(&mut c, 10));
}

#[test]
fn a_black_hole_drops_to_the_floor_and_starts_a_search() {
    let mut c = connection();
    c.mss = 1400;
    c.mss_floor = 536;
    assert!(plpmtud_blackhole(&mut c, 7));
    assert_eq!(c.mss, 536);
    assert_eq!(
        c.plp.as_ref().map(|s| s.state),
        Some(PlpmtudState::Searching)
    );
    assert!(!plpmtud_blackhole(&mut c, 8));
}

#[test]
fn no_black_hole_response_at_the_floor() {
    let mut c = connection();
    c.mss = 536;
    c.mss_floor = 536;
    assert!(!plpmtud_blackhole(&mut c, 0));
    assert!(c.plp.is_none());
}

#[test]
fn a_probe_is_due_only_when_the_room_covers_the_ceiling() {
    let mut c = connection();
    assert_eq!(probe_due(&mut c, 0, 600_000, 800), None);
    assert!(c.plp.is_some());
    assert_eq!(c.plp_next_ms, 200);
    assert_eq!(probe_due(&mut c, 100, 600_000, 4_000), None);
    let (size, reprobe) = probe_due(&mut c, 200, 600_000, 4_000).expect("probe");
    assert!(size > 536 && size < 1460);
    assert!(!reprobe);
    assert_eq!(c.plp_probe, Some((c.snd_nxt.wrapping_add(size), size)));
    assert_eq!(probe_due(&mut c, 300, 600_000, 4_000), None);
}

#[test]
fn no_search_when_the_payload_already_reaches_the_ceiling() {
    let mut c = connection();
    c.mss = 1460;
    assert_eq!(probe_due(&mut c, 0, 600_000, 9_000), None);
    assert!(c.plp.is_none());
    assert_eq!(next_deadline(&c, 600_000, true), None);
}

#[test]
fn a_later_mss_drop_starts_the_search_after_a_full_size_decision() {
    let mut c = connection();
    c.mss = 1460;
    assert_eq!(probe_due(&mut c, 0, 600_000, 9_000), None);
    assert!(c.plp.is_none());
    c.mss = 536;
    c.peer_mss = 1000;
    c.plp_ceiling = 1000;
    let (size, _) = probe_due(&mut c, 0, 600_000, 4_000)
        .expect("a later MSS drop must start the path-MTU search and probe");
    assert!(c.plp.is_some());
    assert!(size > u32::from(c.mss) && size <= 1_000);
}

#[test]
fn a_finished_search_is_probed_again_after_the_interval() {
    let mut c = connection();
    let mut search = Plpmtud::new(536, 1460, 8);
    search.start();
    search.on_probe_loss(537);
    assert_eq!(search.state, PlpmtudState::SearchComplete);
    c.plp = Some(search);
    c.plp_decided = true;
    c.plp_next_ms = 1_000;
    assert_eq!(probe_due(&mut c, 2_000, 600_000, 9_000), None);
    assert_eq!(
        next_deadline(&c, 600_000, true),
        Some(Instant::from_millis(601_000))
    );
    let (_, reprobe) = probe_due(&mut c, 601_000, 600_000, 9_000).expect("reprobe");
    assert!(reprobe);
}

#[test]
fn an_exhausted_search_budget_stops_the_live_timer() {
    let mut c = connection();
    let mut search = Plpmtud::new(536, 1460, 1);
    search.start();
    c.plp = Some(search);
    c.plp_decided = true;
    c.plp_next_ms = 0;
    let (size, _) = probe_due(&mut c, 0, 600_000, 4_000).expect("first probe");
    let end = c.plp_probe.expect("probe recorded").0;
    plpmtud_on_ack(&mut c, end, 1);
    assert!(c.plp_probe.is_none());
    assert_eq!(c.mss, u16::try_from(size).expect("probe fits the MSS field"));

    assert_eq!(probe_due(&mut c, 1, 600_000, 4_000), None);
    assert_eq!(
        c.plp.as_ref().map(|search| search.state),
        Some(PlpmtudState::Error)
    );
    assert_eq!(next_deadline(&c, 600_000, true), None);
}

#[test]
fn netbsd_mss_to_advertise_uses_the_max_mtu_and_default_floor() {
    assert_eq!(
        mss_to_advertise(1500, 9000, IpFamily::V4, false, 536),
        8960
    );
    assert_eq!(mss_to_advertise(1500, 9000, IpFamily::V4, true, 536), 1460);
    assert_eq!(mss_to_advertise(1500, 9000, IpFamily::V6, true, 536), 1440);
    assert_eq!(mss_to_advertise(100, 200, IpFamily::V4, false, 536), 536);
    assert_eq!(mss_to_advertise(0, 0, IpFamily::V4, false, 536), 536);
}

#[test]
fn netbsd_mss_from_peer_clamps_offer_options_and_buffer() {
    let peer = mss_from_peer(0, 536, 0, 0, 8192);
    assert_eq!(peer.peer_mss, 536);
    assert_eq!(peer.segsz, 536);
    assert_eq!(peer.send_buffer, 8576);

    let peer = mss_from_peer(100, 536, 0, 0, 8192);
    assert_eq!(peer.peer_mss, 256);
    assert_eq!(peer.segsz, 256);

    let peer = mss_from_peer(1460, 536, 12, 0, 8192);
    assert_eq!(peer.peer_mss, 1460);
    assert_eq!(peer.segsz, 1448);
    assert_eq!(peer.send_buffer, 8688);

    let peer = mss_from_peer(1460, 536, 0, 0, 1000);
    assert_eq!(peer.segsz, 1000);
    assert_eq!(peer.send_buffer, 1000);

    let peer = mss_from_peer(1460, 536, 2000, 0, 8192);
    assert_eq!(peer.segsz, 0);
    assert_eq!(peer.send_buffer, 8192);
}

#[test]
fn netbsd_icmp_pmtu_classic_pending_and_replacement() {
    let mut path = PmtuPath {
        mtu_sent: 1500,
        ..PmtuPath::default()
    };
    assert_eq!(
        on_icmp_pmtu(&mut path, 100, 100, 200, 1200, 40, 1500, 5),
        PmtuResult::Classic(1200)
    );
    assert!(path.pending.is_none());

    let mut path = PmtuPath {
        mtu_sent: 1500,
        mss_acked: 1300,
        ..PmtuPath::default()
    };
    let first = PmtuPending {
        th_seq: 120,
        nextmtu: 1200,
        ip_len: 1500,
        ip_hl: 5,
    };
    assert_eq!(
        on_icmp_pmtu(&mut path, 120, 100, 200, 1200, 40, 1500, 5),
        PmtuResult::Pending(first)
    );
    assert_eq!(
        on_icmp_pmtu(&mut path, 110, 100, 200, 1100, 40, 1500, 5),
        PmtuResult::Ignored
    );
    assert_eq!(path.pending, Some(first));

    let second = PmtuPending {
        th_seq: 130,
        nextmtu: 1100,
        ip_len: 1500,
        ip_hl: 5,
    };
    assert_eq!(
        on_icmp_pmtu(&mut path, 130, 100, 200, 1100, 40, 1500, 5),
        PmtuResult::Pending(second)
    );
    assert_eq!(
        on_icmp_pmtu(&mut path, 140, 100, 200, 1400, 40, 1500, 5),
        PmtuResult::Classic(1400)
    );
    assert!(path.pending.is_none());
}

#[test]
fn netbsd_icmp_pmtu_drops_stale_and_non_shrinking_reports() {
    let mut path = PmtuPath {
        mtu_sent: 1500,
        ..PmtuPath::default()
    };
    assert_eq!(
        on_icmp_pmtu(&mut path, 99, 100, 200, 1200, 40, 1500, 5),
        PmtuResult::Ignored
    );
    assert_eq!(
        on_icmp_pmtu(&mut path, 200, 100, 200, 1200, 40, 1500, 5),
        PmtuResult::Ignored
    );
    assert_eq!(
        on_icmp_pmtu(&mut path, 150, 100, 200, 1500, 40, 1500, 5),
        PmtuResult::Ignored
    );
}

#[test]
fn netbsd_icmp_pmtu_ack_clears_the_pending_quote() {
    let mut path = PmtuPath {
        mtu_sent: 1500,
        mss_acked: 0,
        pending: Some(PmtuPending {
            th_seq: 120,
            nextmtu: 1200,
            ip_len: 1500,
            ip_hl: 5,
        }),
    };
    path.on_ack(120, 900);
    assert!(path.pending.is_some());
    assert_eq!(path.mss_acked, 900);
    path.on_ack(121, 700);
    assert!(path.pending.is_none());
    assert_eq!(path.mss_acked, 900);
}
