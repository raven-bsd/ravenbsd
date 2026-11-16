// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host tests for the driver release session and its link seam.

use super::*;
use crate::frame::FrameBackend;
use crate::std::string::ToString;
use crate::tests::{FakeSeam, ready_controller};
use raven_genet::phy::{AUXSTS_RES_1000FD, BMSR_ACOMP, BMSR_LINK, Duplex, Speed};
use raven_genet::regs;
use raven_genet::release::{ReleaseError, Stage};

fn backend() -> FrameBackend<FakeSeam> {
    FrameBackend::new(ready_controller()).expect("backend")
}

fn up_1000() -> (u16, u16) {
    (BMSR_LINK | BMSR_ACOMP, AUXSTS_RES_1000FD)
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

fn linked() -> (FrameBackend<FakeSeam>, ReleaseSession) {
    let mut backend = backend();
    let mut session = ReleaseSession::new();
    let (bmsr, auxsts) = up_1000();
    session
        .observe_link(&mut backend, bmsr, auxsts)
        .expect("first sample");
    session
        .observe_link(&mut backend, bmsr, auxsts)
        .expect("second sample");
    (backend, session)
}

#[test]
fn a_new_session_waits_for_the_link() {
    let session = ReleaseSession::new();
    assert_eq!(session.gate().stage(), Stage::Link);
    assert!(session.transcript().is_none());
}

#[test]
fn the_debounced_link_event_advances_the_session() {
    let (backend, session) = linked();
    assert_eq!(session.gate().stage(), Stage::Dhcp);
    assert_eq!(session.gate().link(), Some((Speed::Thousand, Duplex::Full)));
    let command = backend.controller().seam.register(regs::UMAC_CMD);
    assert_eq!(
        (command & regs::UMAC_CMD_SPEED_MASK) >> regs::UMAC_CMD_SPEED_SHIFT,
        regs::UMAC_CMD_SPEED_1000
    );
}

#[test]
fn a_link_down_sample_is_reported_as_a_gate_error() {
    let mut backend = backend();
    let mut session = ReleaseSession::new();
    assert_eq!(
        session
            .observe_link(&mut backend, 0, 0)
            .expect("first sample"),
        None
    );
    assert_eq!(
        session.observe_link(&mut backend, 0, 0),
        Err(DriverError::Release(ReleaseError::LinkDown))
    );
    assert_eq!(session.gate().stage(), Stage::Link);
}

#[test]
fn dhcp_before_the_link_is_reported_as_a_gate_error() {
    let mut session = ReleaseSession::new();
    assert_eq!(
        session.dhcp_bound(lease()),
        Err(DriverError::Release(ReleaseError::WrongStage {
            expected: Stage::Dhcp,
            actual: Stage::Link,
        }))
    );
}

#[test]
fn the_complete_session_formats_the_release_transcript() {
    let (_backend, mut session) = linked();
    session.dhcp_bound(lease()).expect("dhcp");
    session.ping_reply(ping()).expect("ping");
    assert_eq!(session.gate().stage(), Stage::Complete);
    assert_eq!(
        session.transcript().expect("transcript").to_string(),
        "GENET-NET-LINK speed=1000 duplex=full\n\
         GENET-NET-DHCP addr=192.168.1.23/24 gateway=192.168.1.1 dns=192.168.1.1\n\
         GENET-NET-PING target=192.168.1.1 tx=3 rx=3 rtt_ms=0\n\
         GENET-NET-OK\n"
    );
}

#[test]
fn a_ping_to_the_wrong_target_is_reported() {
    let (_backend, mut session) = linked();
    session.dhcp_bound(lease()).expect("dhcp");
    let wrong = PingReport {
        target: [192, 168, 1, 2],
        ..ping()
    };
    assert_eq!(
        session.ping_reply(wrong),
        Err(DriverError::Release(ReleaseError::TargetIsNotGateway {
            target: [192, 168, 1, 2],
            gateway: [192, 168, 1, 1],
        }))
    );
}

#[test]
fn a_ping_without_a_reply_is_reported() {
    let (_backend, mut session) = linked();
    session.dhcp_bound(lease()).expect("dhcp");
    let no_reply = PingReport {
        received: 0,
        ..ping()
    };
    assert_eq!(
        session.ping_reply(no_reply),
        Err(DriverError::Release(ReleaseError::NoReply))
    );
}

#[test]
fn a_partial_session_has_no_transcript() {
    let (_backend, mut session) = linked();
    session.dhcp_bound(lease()).expect("dhcp");
    assert_eq!(session.gate().stage(), Stage::Ping);
    assert!(session.transcript().is_none());
}

#[test]
fn the_default_session_is_new() {
    assert_eq!(ReleaseSession::default(), ReleaseSession::new());
}

#[test]
fn a_release_error_maps_into_the_driver_error() {
    let error: DriverError = ReleaseError::NoReply.into();
    assert_eq!(error, DriverError::Release(ReleaseError::NoReply));
}
