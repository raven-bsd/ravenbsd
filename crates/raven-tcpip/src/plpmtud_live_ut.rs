// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

use super::*;
use crate::Instant;

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
