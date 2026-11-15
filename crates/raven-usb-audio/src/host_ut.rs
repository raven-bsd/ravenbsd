// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Assertions for the device model over the xHCI rings.

use super::*;
use crate::test_config::sample;
use crate::uac1::Uac1Config;

fn host(entries: u16) -> AudioHost {
    AudioHost::new(sample(), entries, 8).expect("host")
}

#[test]
fn the_stream_opens_at_a_supported_rate() {
    let mut host = host(8);
    assert_eq!(host.dci(), 3, "endpoint 1 IN is DCI 3");
    assert!(host.format().is_none());
    assert_eq!(host.queue(0x1000), Err(HostError::NotOpen));
    assert_eq!(host.open(1, 48_000), Ok(()));
    assert_eq!(host.format().map(|format| format.rate_hz), Some(48_000));
    assert_eq!(host.feedback(), Some(0x0006_0000));
    assert_eq!(
        host.next_packet(),
        Some(Packet {
            index: 0,
            frames: 6,
            bytes: 24
        })
    );
}

#[test]
fn opening_checks_the_alternate_and_rate() {
    let mut host = host(8);
    assert_eq!(host.open(0, 48_000), Err(HostError::BadAlternate(0)));
    assert_eq!(
        host.open(1, 96_000),
        Err(HostError::UnsupportedRate(96_000))
    );
    assert_eq!(host.open(1, 44_100), Ok(()));
    assert_eq!(host.feedback(), Some(0x0005_8333));
}

#[test]
fn packets_queue_and_complete_in_order() {
    let mut host = host(8);
    host.open(1, 48_000).expect("open");
    assert_eq!(host.queue(0x1000), Ok(0));
    assert_eq!(host.queue(0x2000), Ok(1));
    assert_eq!(host.queue(0x3000), Ok(2));
    assert_eq!(host.pending(), 3);
    assert_eq!(
        host.next_packet().map(|packet| packet.index),
        Some(3),
        "three packets were produced"
    );

    for expected in 0..3u16 {
        host.device_complete(1).expect("event");
        let completion = host.complete().expect("completion");
        assert_eq!(completion.index, expected);
        assert_eq!(
            completion.trb.parameter,
            0x1000 + u64::from(expected) * 0x1000
        );
        assert_eq!(completion.trb.endpoint(), 3);
        let event = completion.event.expect("transfer event");
        assert_eq!(event.slot(), 1);
        assert_eq!(event.endpoint(), 3);
        assert_eq!(event.completion_code(), 1);
    }
    assert_eq!(host.pending(), 0);
    assert_eq!(host.complete(), None);
}

#[test]
fn a_full_ring_reports_full_and_keeps_the_stream() {
    let mut host = host(4);
    host.open(1, 48_000).expect("open");
    assert_eq!(host.queue(0x1000), Ok(0));
    assert_eq!(host.queue(0x2000), Ok(1));
    assert_eq!(host.queue(0x3000), Ok(2));
    assert_eq!(host.queue(0x4000), Err(HostError::RingFull));
    assert_eq!(
        host.next_packet().map(|packet| packet.index),
        Some(3),
        "a full ring must not consume the packet"
    );
}

#[test]
fn closing_the_stream_stops_it() {
    let mut host = host(8);
    host.open(1, 48_000).expect("open");
    host.close();
    assert!(host.format().is_none());
    assert_eq!(host.feedback(), None);
    assert_eq!(host.queue(0x1000), Err(HostError::NotOpen));
    assert_eq!(host.device_complete(1), Err(HostError::NotOpen));
}

#[test]
fn a_bad_ring_size_is_rejected() {
    let config: Uac1Config = sample();
    assert!(matches!(
        AudioHost::new(config, 3, 8),
        Err(HostError::Ring(RingError::NotPowerOfTwo(3)))
    ));
}
