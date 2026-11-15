// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Assertions for the device model over the xHCI rings.

use super::*;
use crate::control::{FEATURE_MUTE, GET_CUR, SET_CUR, VolumeLimits};
use crate::pcm::{PcmSource, SliceSource};
use crate::test_config::sample;
use crate::uac1::Uac1Config;
use raven_proto_audio::format::{Format, SampleFormat};
use raven_proto_audio::ring::{RingConsumer, RingLayout, RingProducer};
use raven_rp1_xhci::TrbType;

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

fn control_setup(request: u8, selector: u8, length: u16) -> SetupPacket {
    SetupPacket {
        request_type: if request & 0x80 != 0 { 0xA1 } else { 0x21 },
        request,
        value: (2u16 << 8) | u16::from(selector),
        index: 0,
        length,
    }
}

#[test]
fn a_mute_request_queues_the_control_sequence() {
    let mut host = host(8);
    assert_eq!(
        host.control(control_setup(SET_CUR, FEATURE_MUTE, 1), &[1], 0x2000),
        Ok(Response::Ack)
    );
    assert!(host.control_busy());
    assert!(host.controls().mute);
    assert_eq!(
        host.control(control_setup(SET_CUR, FEATURE_MUTE, 1), &[0], 0x2000),
        Err(HostError::ControlBusy)
    );

    host.control_device_complete(1).expect("event");
    let completion = host.control_complete().expect("control completion");
    assert_eq!(completion.trb.kind(), TrbType::StatusStage);
    assert!(completion.event.is_some());
    assert!(!host.control_busy());

    assert_eq!(
        host.control(control_setup(GET_CUR, FEATURE_MUTE, 1), &[], 0x2000),
        Ok(Response::Data {
            bytes: [1, 0],
            len: 1
        })
    );
}

#[test]
fn a_control_transfer_returns_the_device_answer() {
    let mut host = host(8);
    assert_eq!(
        host.control(
            control_setup(SET_CUR, 0x02, 2),
            &(-2048i16).to_le_bytes(),
            0x2000
        ),
        Ok(Response::Ack)
    );
    host.control_device_complete(1).expect("event");
    assert!(host.control_complete().is_some());
    assert_eq!(
        host.control(control_setup(GET_CUR, 0x02, 2), &[], 0x2000),
        Ok(Response::Data {
            bytes: (-2048i16).to_le_bytes(),
            len: 2
        })
    );
    assert_eq!(host.controls().volume, -2048);
}

#[test]
fn a_no_data_control_uses_two_trbs() {
    let mut host = host(8);
    let answer = host
        .control(control_setup(SET_CUR, FEATURE_MUTE, 0), &[], 0x2000)
        .expect("control");
    assert_eq!(answer, Response::Stalled);
    host.control_device_complete(1).expect("event");
    let completion = host.control_complete().expect("completion");
    assert_eq!(completion.trb.kind(), TrbType::StatusStage);
}

#[test]
fn a_small_control_ring_is_rejected() {
    let mut host =
        AudioHost::with_controls(sample(), 8, 8, 2, VolumeLimits::MODEL_DEFAULT).expect("host");
    assert_eq!(
        host.control(control_setup(SET_CUR, FEATURE_MUTE, 1), &[1], 0x2000),
        Err(HostError::RingFull)
    );
    assert!(!host.control_busy());
}

#[test]
fn the_volume_limits_can_be_replaced() {
    let mut host = host(8);
    host.set_volume_limits(VolumeLimits {
        min: -128,
        max: 0,
        res: 1,
    });
    assert_eq!(
        host.control(
            control_setup(SET_CUR, 0x02, 2),
            &(-64i16).to_le_bytes(),
            0x2000
        ),
        Ok(Response::Ack)
    );
}

#[test]
fn pcm_queue_consumes_the_audiod_ring() {
    let format = Format {
        rate: 48_000,
        channels: 2,
        sample: SampleFormat::S16Le,
    };
    let layout = RingLayout::new(128 + 64 * 4, format, 64, 1).expect("layout");
    let mut region = vec![0u8; layout.region_len()];
    layout.write_header(&mut region, 0).expect("header");
    {
        let mut producer = RingProducer::new(&mut region, layout).expect("producer");
        let pcm: Vec<u8> = (0..192u8).collect();
        assert_eq!(producer.write(&pcm), Ok(48), "write returns frames");
    }
    let mut consumer = RingConsumer::new(&mut region, layout).expect("consumer");

    let mut host = AudioHost::new(sample(), 16, 8).expect("host");
    host.open(1, 48_000).expect("open");
    assert_eq!(host.queue_pcm(0x1000, &mut consumer), Ok(0));
    assert_eq!(host.last_packet().len(), 24);
    assert_eq!(host.last_packet()[0], 0);
    assert_eq!(host.last_packet()[23], 23);
    assert_eq!(PcmSource::available_frames(&consumer), 42);
    assert_eq!(host.queue_pcm(0x2000, &mut consumer), Ok(1));
    assert_eq!(PcmSource::available_frames(&consumer), 36);

    for _ in 0..6 {
        assert!(host.queue_pcm(0x3000, &mut consumer).is_ok());
    }
    assert_eq!(PcmSource::available_frames(&consumer), 0);
    assert_eq!(
        host.queue_pcm(0x4000, &mut consumer),
        Err(HostError::SourceUnderrun)
    );
}

#[test]
fn a_pcm_source_with_a_different_frame_size_is_rejected() {
    let mut host = host(8);
    host.open(1, 48_000).expect("open");
    let data = [0u8; 18];
    let mut source = SliceSource::new(&data, 3).expect("source");
    assert_eq!(
        host.queue_pcm(0x1000, &mut source),
        Err(HostError::SourceFormat)
    );
}

#[test]
fn a_short_pcm_source_is_an_underrun() {
    let mut host = host(8);
    host.open(1, 48_000).expect("open");
    let data = [0u8; 8];
    let mut source = SliceSource::new(&data, 4).expect("source");
    assert_eq!(
        host.queue_pcm(0x1000, &mut source),
        Err(HostError::SourceUnderrun)
    );
    assert_eq!(host.pending(), 0);
}
