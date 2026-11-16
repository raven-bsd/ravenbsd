// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host tests for the Pi 4 release network gate and evidence transcript.

use super::*;
use crate::phy::{Duplex, Speed};

fn up_1000() -> LinkEvent {
    LinkEvent::Up {
        speed: Speed::Thousand,
        duplex: Duplex::Full,
    }
}

fn lease() -> Lease {
    Lease::new(
        [192, 168, 1, 23],
        24,
        [192, 168, 1, 1],
        Some([192, 168, 1, 1]),
    )
    .expect("lease")
}

fn ping() -> PingReport {
    PingReport {
        target: [192, 168, 1, 1],
        transmitted: 3,
        received: 3,
        rtt_ms: 0,
    }
}

fn complete() -> ReleaseGate {
    let mut gate = ReleaseGate::new();
    gate.link_event(up_1000()).expect("link");
    gate.dhcp_bound(lease()).expect("dhcp");
    gate.ping_reply(ping()).expect("ping");
    gate
}

#[test]
fn a_new_gate_waits_for_the_link() {
    let gate = ReleaseGate::new();
    assert_eq!(gate.stage(), Stage::Link);
    assert_eq!(gate.link(), None);
    assert_eq!(gate.lease(), None);
    assert_eq!(gate.ping(), None);
    assert!(gate.transcript().is_none());
}

#[test]
fn a_link_up_event_advances_to_dhcp() {
    let mut gate = ReleaseGate::new();
    gate.link_event(up_1000()).expect("link");
    assert_eq!(gate.stage(), Stage::Dhcp);
    assert_eq!(gate.link(), Some((Speed::Thousand, Duplex::Full)));
}

#[test]
fn a_changed_event_is_accepted_as_the_initial_link() {
    let mut gate = ReleaseGate::new();
    gate.link_event(LinkEvent::Changed {
        speed: Speed::Hundred,
        duplex: Duplex::Full,
    })
    .expect("changed");
    assert_eq!(gate.link(), Some((Speed::Hundred, Duplex::Full)));
}

#[test]
fn a_link_down_event_is_rejected() {
    let mut gate = ReleaseGate::new();
    assert_eq!(
        gate.link_event(LinkEvent::Down),
        Err(ReleaseError::LinkDown)
    );
    assert_eq!(gate.stage(), Stage::Link);
}

#[test]
fn dhcp_before_the_link_is_rejected() {
    let mut gate = ReleaseGate::new();
    assert_eq!(
        gate.dhcp_bound(lease()),
        Err(ReleaseError::WrongStage {
            expected: Stage::Dhcp,
            actual: Stage::Link,
        })
    );
}

#[test]
fn ping_before_dhcp_is_rejected() {
    let mut gate = ReleaseGate::new();
    gate.link_event(up_1000()).expect("link");
    assert_eq!(
        gate.ping_reply(ping()),
        Err(ReleaseError::WrongStage {
            expected: Stage::Ping,
            actual: Stage::Dhcp,
        })
    );
}

#[test]
fn a_second_link_event_is_rejected() {
    let mut gate = ReleaseGate::new();
    gate.link_event(up_1000()).expect("link");
    assert_eq!(
        gate.link_event(up_1000()),
        Err(ReleaseError::WrongStage {
            expected: Stage::Link,
            actual: Stage::Dhcp,
        })
    );
}

#[test]
fn invalid_leases_are_rejected() {
    let cases = [
        ([0, 0, 0, 0], 24, [192, 168, 1, 1]),
        ([255, 255, 255, 255], 24, [192, 168, 1, 1]),
        ([224, 0, 0, 1], 24, [192, 168, 1, 1]),
        ([127, 0, 0, 1], 24, [192, 168, 1, 1]),
        ([192, 168, 1, 23], 0, [192, 168, 1, 1]),
        ([192, 168, 1, 23], 33, [192, 168, 1, 1]),
        ([192, 168, 1, 23], 24, [0, 0, 0, 0]),
        ([192, 168, 1, 23], 24, [224, 0, 0, 1]),
        ([192, 168, 1, 23], 24, [192, 168, 1, 23]),
    ];
    for (address, prefix, gateway) in cases {
        assert_eq!(
            Lease::new(address, prefix, gateway, None),
            Err(ReleaseError::InvalidLease),
            "address={address:?} prefix={prefix} gateway={gateway:?}"
        );
    }
}

#[test]
fn an_invalid_dns_server_is_rejected() {
    assert_eq!(
        Lease::new([192, 168, 1, 23], 24, [192, 168, 1, 1], Some([0, 0, 0, 0])),
        Err(ReleaseError::InvalidLease)
    );
}

#[test]
fn a_lease_without_dns_is_accepted() {
    let lease = Lease::new([10, 0, 2, 15], 24, [10, 0, 2, 2], None).expect("lease");
    assert_eq!(lease.dns, None);
}

#[test]
fn the_ping_target_must_be_the_gateway() {
    let mut gate = ReleaseGate::new();
    gate.link_event(up_1000()).expect("link");
    gate.dhcp_bound(lease()).expect("dhcp");
    let wrong = PingReport {
        target: [192, 168, 1, 2],
        ..ping()
    };
    assert_eq!(
        gate.ping_reply(wrong),
        Err(ReleaseError::TargetIsNotGateway {
            target: [192, 168, 1, 2],
            gateway: [192, 168, 1, 1],
        })
    );
}

#[test]
fn a_ping_without_a_reply_is_rejected() {
    let mut gate = ReleaseGate::new();
    gate.link_event(up_1000()).expect("link");
    gate.dhcp_bound(lease()).expect("dhcp");
    let no_reply = PingReport {
        received: 0,
        ..ping()
    };
    assert_eq!(gate.ping_reply(no_reply), Err(ReleaseError::NoReply));
}

#[test]
fn impossible_ping_counts_are_rejected() {
    let mut gate = ReleaseGate::new();
    gate.link_event(up_1000()).expect("link");
    gate.dhcp_bound(lease()).expect("dhcp");
    let zero = PingReport {
        transmitted: 0,
        received: 0,
        ..ping()
    };
    assert_eq!(
        gate.ping_reply(zero),
        Err(ReleaseError::InvalidPing {
            transmitted: 0,
            received: 0,
        })
    );
    let too_many = PingReport {
        transmitted: 2,
        received: 3,
        ..ping()
    };
    assert_eq!(
        gate.ping_reply(too_many),
        Err(ReleaseError::InvalidPing {
            transmitted: 2,
            received: 3,
        })
    );
}

#[test]
fn a_complete_gate_formats_the_stable_markers() {
    let transcript = complete().transcript().expect("transcript");
    assert_eq!(
        transcript.to_string(),
        "GENET-NET-LINK speed=1000 duplex=full\n\
         GENET-NET-DHCP addr=192.168.1.23/24 gateway=192.168.1.1 dns=192.168.1.1\n\
         GENET-NET-PING target=192.168.1.1 tx=3 rx=3 rtt_ms=0\n\
         GENET-NET-OK\n"
    );
}

#[test]
fn a_lease_without_dns_prints_none() {
    let transcript = ReleaseTranscript {
        speed: Speed::Ten,
        duplex: Duplex::Half,
        lease: Lease::new([10, 0, 2, 15], 24, [10, 0, 2, 2], None).expect("lease"),
        ping: PingReport {
            target: [10, 0, 2, 2],
            transmitted: 1,
            received: 1,
            rtt_ms: 3,
        },
    };
    let text = transcript.to_string();
    assert!(text.contains("GENET-NET-LINK speed=10 duplex=half\n"));
    assert!(text.contains("addr=10.0.2.15/24 gateway=10.0.2.2 dns=none\n"));
    assert!(text.contains("GENET-NET-PING target=10.0.2.2 tx=1 rx=1 rtt_ms=3\n"));
}

#[test]
fn a_partial_gate_has_no_transcript() {
    let mut gate = ReleaseGate::new();
    gate.link_event(up_1000()).expect("link");
    gate.dhcp_bound(lease()).expect("dhcp");
    assert_eq!(gate.stage(), Stage::Ping);
    assert!(gate.transcript().is_none());
}

#[test]
fn the_default_gate_is_new() {
    assert_eq!(ReleaseGate::default(), ReleaseGate::new());
}

#[test]
fn the_marker_list_is_the_transcript_order() {
    assert_eq!(MARKERS, [MARKER_LINK, MARKER_DHCP, MARKER_PING, MARKER_OK]);
}
