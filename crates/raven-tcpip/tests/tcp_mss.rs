// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! K303: FreeBSD `tcp_maxseg`/`tcp_fixed_maxseg` and the MSS clamp.

use raven_tcpip::{IpFamily, MssOptions, MssState, mss_from_peer, tcp_fixed_maxseg, tcp_maxseg};

fn options() -> MssOptions {
    MssOptions::default()
}

#[test]
fn ipv4_peer_mss_is_bounded_by_the_path_mtu() {
    assert_eq!(mss_from_peer(IpFamily::V4, 1500, Some(1460)), 1460);
    assert_eq!(mss_from_peer(IpFamily::V4, 1500, Some(9000)), 1460);
}

#[test]
fn a_missing_mss_option_uses_the_family_default() {
    assert_eq!(mss_from_peer(IpFamily::V4, 1500, None), 536);
    assert_eq!(mss_from_peer(IpFamily::V6, 1500, None), 1220);
}

#[test]
fn a_tiny_peer_mss_is_rounded_up_to_tcp_minmss() {
    assert_eq!(mss_from_peer(IpFamily::V4, 1500, Some(100)), 216);
}

#[test]
fn ipv6_subtracts_the_larger_header() {
    assert_eq!(mss_from_peer(IpFamily::V6, 1280, Some(1440)), 1220);
    assert_eq!(mss_from_peer(IpFamily::V6, 1500, Some(1440)), 1440);
}

#[test]
fn a_path_mtu_below_the_minimum_still_leaves_sixty_four_bytes() {
    assert_eq!(mss_from_peer(IpFamily::V4, 100, Some(1460)), 64);
}

#[test]
fn no_options_returns_the_negotiated_mss() {
    let mut opts = options();
    opts.no_options = true;
    assert_eq!(tcp_maxseg(1460, MssState::Established, &opts), 1460);
    assert_eq!(tcp_fixed_maxseg(1460, MssState::Established, &opts), 1460);
}

#[test]
fn established_timestamps_cost_twelve_bytes() {
    let mut opts = options();
    opts.received_timestamp = true;
    assert_eq!(tcp_maxseg(1460, MssState::Established, &opts), 1448);
    assert_eq!(tcp_fixed_maxseg(1460, MssState::Established, &opts), 1448);
}

#[test]
fn established_sack_blocks_cost_twenty_bytes() {
    let mut opts = options();
    opts.received_timestamp = true;
    opts.sack_permitted = true;
    opts.received_sack_blocks = 2;
    // 12-byte timestamp + 2-byte SACK header + 2*8-byte blocks = 30, padded to 32.
    assert_eq!(tcp_maxseg(1460, MssState::Established, &opts), 1428);
    // The fixed form deliberately ignores the variable SACK blocks.
    assert_eq!(tcp_fixed_maxseg(1460, MssState::Established, &opts), 1448);
}

#[test]
fn established_sack_options_are_clamped_to_forty_bytes() {
    let mut opts = options();
    opts.received_timestamp = true;
    opts.sack_permitted = true;
    opts.received_sack_blocks = 9;
    assert_eq!(tcp_maxseg(1460, MssState::Established, &opts), 1420);
}

#[test]
fn handshake_options_match_the_syn_option_shape() {
    let mut opts = options();
    opts.requested_timestamp = true;
    opts.requested_window_scale = true;
    opts.sack_permitted = true;
    // Timestamp 12 + window scale 4 + SACK permitted 4 = 20.
    assert_eq!(tcp_maxseg(1460, MssState::Handshake, &opts), 1440);
    assert_eq!(tcp_fixed_maxseg(1460, MssState::Handshake, &opts), 1440);
}

#[test]
fn handshake_without_timestamps_still_pays_for_the_mss_option() {
    let opts = options();
    // PADTCPOLEN(TCPOLEN_MAXSEG) = 4.
    assert_eq!(tcp_maxseg(1460, MssState::Handshake, &opts), 1456);
}

#[test]
fn signature_and_all_fixed_options_hit_the_forty_byte_ceiling() {
    let mut opts = options();
    opts.requested_timestamp = true;
    opts.requested_window_scale = true;
    opts.sack_permitted = true;
    opts.signature = true;
    // 12 + 4 + 4 + 20 = 40.
    assert_eq!(tcp_maxseg(1460, MssState::Handshake, &opts), 1420);
    assert_eq!(tcp_fixed_maxseg(1460, MssState::Handshake, &opts), 1420);
}

#[test]
fn effective_mss_saturates_instead_of_wrapping() {
    let mut opts = options();
    opts.received_timestamp = true;
    opts.sack_permitted = true;
    opts.received_sack_blocks = 9;
    assert_eq!(tcp_maxseg(20, MssState::Established, &opts), 0);
}
