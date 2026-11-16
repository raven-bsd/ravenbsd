// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Assertions for the isochronous stream schedule.

use super::*;

fn format(rate_hz: u32) -> PcmFormat {
    PcmFormat {
        channels: 2,
        subframe_bytes: 2,
        bit_resolution: 16,
        rate_hz,
    }
}

#[test]
fn the_pcm_format_is_validated() {
    assert!(format(48_000).validate().is_ok());
    assert_eq!(
        PcmFormat {
            channels: 0,
            ..format(48_000)
        }
        .validate(),
        Err(StreamError::BadFormat("channel count"))
    );
    assert_eq!(
        PcmFormat {
            subframe_bytes: 0,
            ..format(48_000)
        }
        .validate(),
        Err(StreamError::BadFormat("subframe size"))
    );
    assert_eq!(
        PcmFormat {
            bit_resolution: 33,
            ..format(48_000)
        }
        .validate(),
        Err(StreamError::BadFormat("bit resolution"))
    );
    assert_eq!(
        format(7_999).validate(),
        Err(StreamError::BadFormat("sample rate"))
    );
    assert_eq!(
        format(192_001).validate(),
        Err(StreamError::BadFormat("sample rate"))
    );
}

#[test]
fn forty_eight_kilohertz_sends_six_frames_per_microframe() {
    let schedule = Schedule::new(format(48_000), HIGH_SPEED_MICROFRAMES, 0x00C0).expect("schedule");
    for index in 0..16 {
        assert_eq!(schedule.frames_in(index), 6);
        assert_eq!(
            schedule.packet(index),
            Ok(Packet {
                index,
                frames: 6,
                bytes: 24
            })
        );
    }
    assert_eq!(schedule.feedback().expect("feedback").raw(), 0x0006_0000);
}

#[test]
fn forty_four_one_distributes_the_fraction() {
    let schedule = Schedule::new(format(44_100), HIGH_SPEED_MICROFRAMES, 0x00C0).expect("schedule");
    let mut frames = 0u64;
    let mut bytes = 0u64;
    for index in 0..HIGH_SPEED_MICROFRAMES {
        let packet = schedule.packet(index).expect("packet");
        assert!(packet.frames == 5 || packet.frames == 6);
        frames = frames.saturating_add(u64::from(packet.frames));
        bytes = bytes.saturating_add(u64::from(packet.bytes));
    }
    assert_eq!(frames, 44_100);
    assert_eq!(bytes, 44_100 * 4);
    assert_eq!(schedule.feedback().expect("feedback").raw(), 0x0005_8333);
}

#[test]
fn a_full_speed_schedule_uses_frames() {
    let schedule = Schedule::new(format(48_000), FULL_SPEED_FRAMES, 0x0200).expect("schedule");
    assert_eq!(schedule.frames_in(0), 48);
    assert_eq!(schedule.bytes_in(0), 192);
    assert_eq!(schedule.feedback().expect("feedback").raw(), 0x0030_0000);
}

#[test]
fn a_packet_above_the_endpoint_limit_is_rejected() {
    let schedule = Schedule::new(format(48_000), HIGH_SPEED_MICROFRAMES, 20).expect("schedule");
    assert_eq!(
        schedule.packet(0),
        Err(StreamError::PacketTooLarge { bytes: 24, max: 20 })
    );
}

#[test]
fn invalid_schedule_parameters_are_rejected() {
    assert_eq!(
        Schedule::new(format(48_000), 0, 64),
        Err(StreamError::BadFrameRate(0))
    );
    assert_eq!(
        Schedule::new(format(48_000), HIGH_SPEED_MICROFRAMES, 0),
        Err(StreamError::ZeroMaxPacket)
    );
    assert_eq!(
        Schedule::new(format(0), HIGH_SPEED_MICROFRAMES, 64).err(),
        Some(StreamError::BadFormat("sample rate"))
    );
}

#[test]
fn the_stream_advances_only_after_start() {
    let schedule = Schedule::new(format(48_000), HIGH_SPEED_MICROFRAMES, 0x00C0).expect("schedule");
    let mut stream = IsoStream::new(schedule);
    assert_eq!(stream.next_packet(), Err(StreamError::NotRunning));
    stream.start();
    assert!(stream.is_running());
    assert_eq!(stream.next_index(), 0);
    let first = stream.next_packet().expect("first");
    assert_eq!(first.index, 0);
    assert_eq!(first.bytes, 24);
    let second = stream.next_packet().expect("second");
    assert_eq!(second.index, 1);
    assert_eq!(stream.next_index(), 2);
    stream.stop();
    assert_eq!(stream.next_packet(), Err(StreamError::NotRunning));
}

#[test]
fn the_feedback_value_is_four_little_endian_bytes() {
    let feedback = Feedback::from_rate(48_000, HIGH_SPEED_MICROFRAMES).expect("feedback");
    assert_eq!(feedback.to_bytes(), [0x00, 0x00, 0x06, 0x00]);
    assert_eq!(
        Feedback::from_rate(48_000, 0),
        Err(StreamError::BadFrameRate(0))
    );
}
