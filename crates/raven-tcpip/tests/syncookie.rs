// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! FreeBSD TCP SYN cookie vectors and policy (K304).

use raven_tcpip::{
    CookieAck, CookieConn, CookieRequest, IpAddr, Ipv4Addr, Ipv6Addr, SyncacheSnapshot, Syncookie,
    SyncookiePolicy,
};

const KEY0: [u8; 16] = [
    0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0a, 0x0b, 0x0c, 0x0d, 0x0e, 0x0f,
];
const KEY1: [u8; 16] = [
    0x10, 0x11, 0x12, 0x13, 0x14, 0x15, 0x16, 0x17, 0x18, 0x19, 0x1a, 0x1b, 0x1c, 0x1d, 0x1e, 0x1f,
];
const SECMOD: u64 = 0x1234_5678_9abc_def0;

fn v4_conn() -> CookieConn {
    CookieConn::new(
        IpAddr::V4(Ipv4Addr::new([192, 0, 2, 1])),
        40_000,
        IpAddr::V4(Ipv4Addr::new([198, 51, 100, 2])),
        80,
    )
}

fn v6_conn() -> CookieConn {
    CookieConn::new(
        IpAddr::V6(Ipv6Addr::new([
            0x20, 0x01, 0x0d, 0xb8, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1,
        ])),
        40_000,
        IpAddr::V6(Ipv6Addr::new([
            0x20, 0x01, 0x0d, 0xb8, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 2,
        ])),
        443,
    )
}

fn request(peer_mss: u16, wscale: Option<u8>, sack: bool) -> CookieRequest {
    CookieRequest {
        peer_mss,
        requested_s_scale: wscale,
        sack_permitted: sack,
    }
}

#[test]
fn the_donor_vector_is_stable() {
    let cookie = Syncookie::new(0, KEY0, KEY1);
    let iss = cookie.generate(
        &v4_conn(),
        0x1122_3344,
        &request(1_460, Some(8), true),
        SECMOD,
    );
    assert_eq!(iss, 0xfc0a_cc02);

    let expanded = cookie
        .expand(
            &v4_conn(),
            &CookieAck {
                ack: 0xfc0a_cc03,
                seq: 0x1122_3345,
                window: 65_535,
            },
            SECMOD,
        )
        .expect("valid cookie");
    assert_eq!(expanded.peer_mss, 1_460);
    assert_eq!(expanded.requested_s_scale, Some(8));
    assert!(expanded.sack_permitted);
    assert_eq!(expanded.window, 65_535);
    assert_eq!(expanded.irs, 0x1122_3344);
    assert_eq!(expanded.iss, 0xfc0a_cc02);
}

#[test]
fn a_cookie_from_another_connection_fails() {
    let cookie = Syncookie::new(0, KEY0, KEY1);
    let iss = cookie.generate(
        &v4_conn(),
        0x1122_3344,
        &request(1_460, None, false),
        SECMOD,
    );
    let ack = CookieAck {
        ack: iss.wrapping_add(1),
        seq: 0x1122_3345,
        window: 10_000,
    };

    let mut other_port = v4_conn();
    other_port.remote_port = 443;
    assert!(cookie.expand(&other_port, &ack, SECMOD).is_none());

    let mut other_address = v4_conn();
    other_address.remote = IpAddr::V4(Ipv4Addr::new([198, 51, 100, 9]));
    assert!(cookie.expand(&other_address, &ack, SECMOD).is_none());

    assert!(
        cookie
            .expand(&v4_conn(), &ack, SECMOD.wrapping_add(1))
            .is_none()
    );
}

#[test]
fn a_cookie_from_another_irs_fails() {
    let cookie = Syncookie::new(0, KEY0, KEY1);
    let iss = cookie.generate(
        &v4_conn(),
        0x1122_3344,
        &request(1_460, None, false),
        SECMOD,
    );
    let wrong_seq = CookieAck {
        ack: iss.wrapping_add(1),
        seq: 0x1122_3346,
        window: 10_000,
    };
    assert!(cookie.expand(&v4_conn(), &wrong_seq, SECMOD).is_none());
}

#[test]
fn a_tampered_ack_fails() {
    let cookie = Syncookie::new(0, KEY0, KEY1);
    let iss = cookie.generate(
        &v4_conn(),
        0x1122_3344,
        &request(1_460, None, false),
        SECMOD,
    );
    for delta in [1u32, 0x100, 0x0100_0000] {
        let tampered = CookieAck {
            ack: iss.wrapping_add(1).wrapping_add(delta),
            seq: 0x1122_3345,
            window: 10_000,
        };
        assert!(cookie.expand(&v4_conn(), &tampered, SECMOD).is_none());
    }
}

#[test]
fn mss_uses_the_donor_table_and_rounds_down() {
    let cookie = Syncookie::new(0, KEY0, KEY1);
    for (peer, expected) in [
        (100u16, 216u16),
        (216, 216),
        (536, 536),
        (1_401, 1_400),
        (9_000, 1_460),
    ] {
        let iss = cookie.generate(&v4_conn(), 0x1122_3344, &request(peer, None, false), SECMOD);
        let expanded = cookie
            .expand(
                &v4_conn(),
                &CookieAck {
                    ack: iss.wrapping_add(1),
                    seq: 0x1122_3345,
                    window: 10_000,
                },
                SECMOD,
            )
            .expect("valid cookie");
        assert_eq!(expanded.peer_mss, expected, "peer_mss {peer}");
    }
}

#[test]
fn wscale_uses_the_donor_table_and_sack_is_one_bit() {
    let cookie = Syncookie::new(0, KEY0, KEY1);
    for (scale, expected) in [
        (0u8, Some(0)),
        (1, Some(1)),
        (3, Some(2)),
        (5, Some(4)),
        (14, Some(8)),
    ] {
        let iss = cookie.generate(
            &v4_conn(),
            0x1122_3344,
            &request(1_460, Some(scale), true),
            SECMOD,
        );
        let expanded = cookie
            .expand(
                &v4_conn(),
                &CookieAck {
                    ack: iss.wrapping_add(1),
                    seq: 0x1122_3345,
                    window: 10_000,
                },
                SECMOD,
            )
            .expect("valid cookie");
        assert_eq!(expanded.requested_s_scale, expected, "scale {scale}");
        assert!(expanded.sack_permitted, "scale {scale}");
    }

    let iss = cookie.generate(
        &v4_conn(),
        0x1122_3344,
        &request(1_460, None, false),
        SECMOD,
    );
    let expanded = cookie
        .expand(
            &v4_conn(),
            &CookieAck {
                ack: iss.wrapping_add(1),
                seq: 0x1122_3345,
                window: 10_000,
            },
            SECMOD,
        )
        .expect("valid cookie");
    assert_eq!(expanded.requested_s_scale, None);
    assert!(!expanded.sack_permitted);
}

#[test]
fn ipv6_connections_use_sixteen_bytes_per_address() {
    let cookie = Syncookie::new(0, KEY0, KEY1);
    let iss = cookie.generate(
        &v6_conn(),
        0x1122_3344,
        &request(1_460, None, false),
        SECMOD,
    );
    let expanded = cookie
        .expand(
            &v6_conn(),
            &CookieAck {
                ack: iss.wrapping_add(1),
                seq: 0x1122_3345,
                window: 10_000,
            },
            SECMOD,
        )
        .expect("valid cookie");
    assert_eq!(expanded.peer_mss, 1_460);
    assert!(
        cookie
            .expand(
                &v4_conn(),
                &CookieAck {
                    ack: iss.wrapping_add(1),
                    seq: 0x1122_3345,
                    window: 10_000,
                },
                SECMOD,
            )
            .is_none()
    );
}

#[test]
fn reseeding_rotates_both_secrets_and_old_cookies_eventually_fail() {
    let mut cookie = Syncookie::new(0, KEY0, KEY1);
    let old_iss = cookie.generate(
        &v4_conn(),
        0x1122_3344,
        &request(1_460, None, false),
        SECMOD,
    );
    let old_ack = CookieAck {
        ack: old_iss.wrapping_add(1),
        seq: 0x1122_3345,
        window: 10_000,
    };
    assert!(cookie.expand(&v4_conn(), &old_ack, SECMOD).is_some());

    cookie.reseed(15_000, [0x55; 16]);
    assert!(
        cookie.expand(&v4_conn(), &old_ack, SECMOD).is_some(),
        "the old key is still one of the two rotating secrets"
    );

    cookie.reseed(30_000, [0xaa; 16]);
    assert!(
        cookie.expand(&v4_conn(), &old_ack, SECMOD).is_none(),
        "the old key was overwritten"
    );
    let new_iss = cookie.generate(
        &v4_conn(),
        0x1122_3344,
        &request(1_460, None, false),
        SECMOD,
    );
    assert!(
        cookie
            .expand(
                &v4_conn(),
                &CookieAck {
                    ack: new_iss.wrapping_add(1),
                    seq: 0x1122_3345,
                    window: 10_000,
                },
                SECMOD,
            )
            .is_some()
    );
}

#[test]
fn the_reseed_interval_is_fifteen_seconds() {
    let mut cookie = Syncookie::new(0, KEY0, KEY1);
    assert!(!cookie.reseed_due(14_999));
    assert!(cookie.reseed_due(15_000));
    cookie.reseed(15_000, [0x55; 16]);
    assert!(!cookie.reseed_due(29_999));
    assert!(cookie.reseed_due(30_000));
}

#[test]
fn the_policy_matches_the_syncookies_and_syncookies_only_flags() {
    let default = SyncookiePolicy::default();
    assert!(!default.cookie_on_overflow(false));
    assert!(default.cookie_on_overflow(true));
    assert!(default.syncache_allowed());

    let disabled = SyncookiePolicy {
        enabled: false,
        only: false,
    };
    assert!(!disabled.cookie_on_overflow(true));
    assert!(disabled.syncache_allowed());

    let only = SyncookiePolicy {
        enabled: false,
        only: true,
    };
    assert!(only.cookie_on_overflow(false));
    assert!(!only.syncache_allowed());
}

#[test]
fn cmp_matches_the_reconstructed_and_syncache_values() {
    let cookie = Syncookie::new(0, KEY0, KEY1);
    let iss = cookie.generate(
        &v4_conn(),
        0x1122_3344,
        &request(1_460, Some(8), true),
        SECMOD,
    );
    let expanded = cookie
        .expand(
            &v4_conn(),
            &CookieAck {
                ack: iss.wrapping_add(1),
                seq: 0x1122_3345,
                window: 10_000,
            },
            SECMOD,
        )
        .expect("valid cookie");
    let same = SyncacheSnapshot {
        peer_mss: 1_460,
        requested_s_scale: Some(8),
        sack_permitted: true,
    };
    assert!(raven_tcpip::syncookie_cmp(&expanded, &same));
    let different = SyncacheSnapshot {
        peer_mss: 1_400,
        ..same
    };
    assert!(!raven_tcpip::syncookie_cmp(&expanded, &different));
}

#[test]
fn the_window_is_carried_through_expansion() {
    let cookie = Syncookie::new(0, KEY0, KEY1);
    let iss = cookie.generate(
        &v4_conn(),
        0x1122_3344,
        &request(1_460, None, false),
        SECMOD,
    );
    for window in [0u16, 65_535] {
        let expanded = cookie
            .expand(
                &v4_conn(),
                &CookieAck {
                    ack: iss.wrapping_add(1),
                    seq: 0x1122_3345,
                    window,
                },
                SECMOD,
            )
            .expect("valid cookie");
        assert_eq!(expanded.window, window);
    }
}
