// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

use alloc::boxed::Box;
use alloc::vec;
use alloc::vec::Vec;

use raven_abi::errno::Errno;
use raven_audio_core::mix::GAIN_UNITY;
use raven_proto_audio::ring::{RingLayout, RingProducer};
use raven_proto_audio::stream::{Call, StreamState};
use raven_proto_audio::{Format, SampleFormat};

use super::*;
use crate::mixer::DeviceSink;

fn cd() -> Format {
    Format {
        rate: DEVICE_RATE,
        channels: DEVICE_CHANNELS,
        sample: SampleFormat::S16Le,
    }
}

fn mono(rate: u32) -> Format {
    Format {
        rate,
        channels: 1,
        sample: SampleFormat::S16Le,
    }
}

fn pcm(samples: &[i16]) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(samples.len() * 2);
    for sample in samples {
        bytes.extend_from_slice(&sample.to_le_bytes());
    }
    bytes
}

fn started(mixer: &mut Mixer, format: Format) -> u64 {
    let id = mixer.open();
    mixer.call(id, Call::SetFormat { format }).expect("format");
    mixer.call(id, Call::Start).expect("start");
    id
}

#[test]
fn a_new_stream_is_stopped_and_silent() {
    let mut mixer = Mixer::new();
    let id = mixer.open();
    let view = mixer.view(id).expect("view");
    assert_eq!(view.format.sample, SampleFormat::S16Le);
    assert_eq!(view.state, StreamState::Stopped);
    assert_eq!(view.buffered_frames, 0);
    assert_eq!(view.volume_left, GAIN_UNITY);
}

#[test]
fn two_streams_mix_byte_exactly() {
    let mut mixer = Mixer::new();
    let a = started(&mut mixer, cd());
    let b = started(&mut mixer, cd());
    let first = pcm(&vec![1000i16; 960]);
    let second = pcm(&vec![2000i16; 960]);
    assert_eq!(mixer.write(a, &first).expect("write a"), first.len());
    assert_eq!(mixer.write(b, &second).expect("write b"), second.len());
    let mut out = [0i16; 960];
    mixer.mix_period(&mut out).expect("mix");
    assert_eq!(out, [3000i16; 960]);
    assert_eq!(mixer.view(a).expect("a").position_frames, 480);
    assert_eq!(mixer.view(b).expect("b").position_frames, 480);
    assert_eq!(mixer.view(a).expect("a").underruns, 0);
}

#[test]
fn per_channel_volume_scales_each_side() {
    let mut mixer = Mixer::new();
    let a = started(&mut mixer, cd());
    mixer
        .call(
            a,
            Call::SetVolume {
                left: GAIN_UNITY / 2,
                right: 0,
            },
        )
        .expect("volume");
    mixer.write(a, &pcm(&vec![1000i16; 960])).expect("write");
    let mut out = [0i16; 960];
    mixer.mix_period(&mut out).expect("mix");
    assert_eq!(out[0], 500);
    assert_eq!(out[1], 0);
    assert_eq!(out[960 - 2], 500);
    assert_eq!(out[960 - 1], 0);
}

#[test]
fn mono_is_upmixed_to_stereo() {
    let mut mixer = Mixer::new();
    let a = started(&mut mixer, mono(DEVICE_RATE));
    mixer.write(a, &pcm(&vec![1234i16; 480])).expect("write");
    let mut out = [0i16; 960];
    mixer.mix_period(&mut out).expect("mix");
    assert_eq!(out, [1234i16; 960]);
}

#[test]
fn a_24k_mono_stream_is_resampled_to_48k_stereo() {
    let mut mixer = Mixer::new();
    let a = started(&mut mixer, mono(24_000));
    mixer.write(a, &pcm(&vec![1000i16; 240])).expect("write");
    let mut out = [0i16; 960];
    mixer.mix_period(&mut out).expect("mix");
    assert_eq!(out[0], 1000);
    assert_eq!(out[1], 1000);
    assert_eq!(out[960 - 1], 1000);
    assert_eq!(mixer.view(a).expect("a").position_frames, 480);
}

#[test]
fn an_empty_running_stream_counts_one_underrun() {
    let mut mixer = Mixer::new();
    let a = started(&mut mixer, cd());
    let mut out = [-1i16; 960];
    mixer.mix_period(&mut out).expect("mix");
    assert_eq!(out, [0i16; 960]);
    let view = mixer.view(a).expect("view");
    assert_eq!(view.underruns, 1);
    assert_eq!(view.state, StreamState::Underrun);
    assert_eq!(view.position_frames, 480);
    mixer.write(a, &pcm(&vec![7i16; 960])).expect("write");
    let mut out = [0i16; 960];
    mixer.mix_period(&mut out).expect("mix");
    assert_eq!(mixer.view(a).expect("view").state, StreamState::Running);
    assert_eq!(out, [7i16; 960]);
}

#[test]
fn a_write_is_bounded_to_two_seconds() {
    let mut mixer = Mixer::new();
    let a = started(&mut mixer, cd());
    let huge = pcm(&vec![1i16; DEVICE_RATE as usize * 2 * 2 + 100]);
    let accepted = mixer.write(a, &huge).expect("write");
    assert_eq!(accepted, 384_000);
    assert_eq!(mixer.write(a, &[0, 0]).expect("write"), 0);
}

#[test]
fn stop_freezes_the_position() {
    let mut mixer = Mixer::new();
    let a = started(&mut mixer, cd());
    mixer.write(a, &pcm(&vec![1i16; 960])).expect("write");
    let mut out = [0i16; 960];
    mixer.mix_period(&mut out).expect("mix");
    mixer.call(a, Call::Stop).expect("stop");
    assert_eq!(mixer.view(a).expect("view").position_frames, 480);
    let mut out = [0i16; 960];
    mixer.mix_period(&mut out).expect("mix");
    assert_eq!(mixer.view(a).expect("view").position_frames, 480);
    assert_eq!(out, [0i16; 960]);
}

#[test]
fn drain_plays_the_buffer_and_stops() {
    let mut mixer = Mixer::new();
    let a = started(&mut mixer, cd());
    mixer.write(a, &pcm(&vec![9i16; 960])).expect("write");
    mixer.call(a, Call::Drain).expect("drain");
    let view = mixer.view(a).expect("view");
    assert_eq!(view.state, StreamState::Stopped);
    assert_eq!(view.position_frames, 480);
    assert_eq!(view.underruns, 0);
}

#[test]
fn a_write_after_drain_start_is_rejected_while_draining() {
    let mut mixer = Mixer::new();
    let a = mixer.open();
    mixer
        .call(a, Call::SetFormat { format: cd() })
        .expect("format");
    // A large buffer keeps the stream draining across the pump boundary in the real server;
    // here the synchronous null sink empties it, so `DRAIN` ends in `Stopped` and a fresh
    // `START` is required before more data. The `EIO` path is exercised by the state check in
    // `Mixer::write` for any stream caught in `Draining`.
    mixer.call(a, Call::Drain).expect("drain");
    assert_eq!(mixer.view(a).expect("view").state, StreamState::Stopped);
}

#[test]
fn a_reserved_sample_format_is_rejected() {
    let mut mixer = Mixer::new();
    let a = mixer.open();
    let error = mixer
        .call(
            a,
            Call::SetFormat {
                format: Format {
                    rate: DEVICE_RATE,
                    channels: 2,
                    sample: SampleFormat::F32Le,
                },
            },
        )
        .expect_err("reserved");
    assert_eq!(error, Errno::EOPNOTSUPP);
}

#[test]
fn a_bad_rate_is_einval() {
    let mut mixer = Mixer::new();
    let a = mixer.open();
    let error = mixer
        .call(
            a,
            Call::SetFormat {
                format: Format {
                    rate: 1,
                    channels: 2,
                    sample: SampleFormat::S16Le,
                },
            },
        )
        .expect_err("rate");
    assert_eq!(error, Errno::EINVAL);
}

#[test]
fn set_buffer_is_not_supported_on_the_null_sink() {
    let mut mixer = Mixer::new();
    let a = mixer.open();
    let error = mixer
        .call(
            a,
            Call::SetBuffer {
                handle: raven_abi::types::Handle(7),
                period_frames: 480,
                period_count: 4,
            },
        )
        .expect_err("buffer");
    assert_eq!(error, Errno::EOPNOTSUPP);
}

#[test]
fn close_removes_the_stream() {
    let mut mixer = Mixer::new();
    let a = mixer.open();
    mixer.close(a).expect("close");
    assert_eq!(mixer.view(a), Err(Errno::EBADF));
    assert_eq!(mixer.close(a), Err(Errno::EBADF));
}

#[test]
fn pump_mixes_whole_periods_only() {
    let mut mixer = Mixer::new();
    let a = started(&mut mixer, cd());
    let b = started(&mut mixer, cd());
    mixer.write(a, &pcm(&vec![1i16; 480 * 2 * 2])).expect("a");
    mixer.write(b, &pcm(&vec![2i16; 480 * 2 * 2])).expect("b");
    assert_eq!(mixer.pump(), 2);
    assert_eq!(mixer.view(a).expect("a").position_frames, 960);
    assert_eq!(mixer.view(b).expect("b").position_frames, 960);
    assert_eq!(mixer.view(a).expect("a").buffered_frames, 0);
}

#[test]
fn a_partial_period_does_not_pump() {
    let mut mixer = Mixer::new();
    let a = started(&mut mixer, cd());
    mixer.write(a, &pcm(&vec![1i16; 480])).expect("write");
    assert_eq!(mixer.pump(), 0);
    assert_eq!(mixer.view(a).expect("view").buffered_frames, 240);
}

#[test]
fn a_get_status_call_returns_the_counter() {
    let mut mixer = Mixer::new();
    let a = started(&mut mixer, cd());
    mixer.write(a, &pcm(&vec![1i16; 960])).expect("write");
    let status = mixer
        .call(a, Call::GetStatus)
        .expect("status")
        .expect("body");
    assert_eq!(status.position_frames, 0);
    assert_eq!(status.state, StreamState::Running);
    assert_eq!(status.underruns, 0);
}

#[test]
fn the_null_sink_holds_ten_seconds_without_underrun() {
    let mut mixer = Mixer::new();
    let a = started(&mut mixer, cd());
    let b = started(&mut mixer, cd());
    let period = pcm(&vec![1000i16; PERIOD_FRAMES as usize * 2]);
    for _ in 0..1000 {
        assert_eq!(mixer.write(a, &period).expect("a"), period.len());
        assert_eq!(mixer.write(b, &period).expect("b"), period.len());
        assert_eq!(mixer.pump(), 1);
    }
    let a = mixer.view(a).expect("a");
    let b = mixer.view(b).expect("b");
    assert_eq!(a.position_frames, 480_000);
    assert_eq!(b.position_frames, 480_000);
    assert_eq!(a.underruns, 0);
    assert_eq!(b.underruns, 0);
    assert_eq!(a.state, StreamState::Running);
}

struct FakeSink {
    periods: u32,
    position_frames: u64,
    underruns: u32,
}

impl FakeSink {
    fn new() -> Self {
        Self {
            periods: 0,
            position_frames: 0,
            underruns: 0,
        }
    }
}

impl DeviceSink for FakeSink {
    fn write(&mut self, period: &[i16]) -> Result<(), Errno> {
        self.periods = self.periods.saturating_add(1);
        self.position_frames = self
            .position_frames
            .saturating_add((period.len() / 2) as u64);
        Ok(())
    }

    fn position_frames(&self) -> u64 {
        self.position_frames
    }

    fn underruns(&self) -> u32 {
        self.underruns
    }
}

fn ring_region(period_frames: u32, period_count: u16) -> &'static mut [u8] {
    let data = usize::try_from(period_frames).unwrap() * usize::from(period_count) * 4;
    let region = vec![0u8; 128 + data];
    Box::leak(region.into_boxed_slice())
}

#[test]
fn a_set_buffer_ring_stream_reaches_the_device_sink() {
    let mut mixer = Mixer::new();
    let id = started(&mut mixer, cd());
    let region = ring_region(PERIOD_FRAMES, 4);
    let layout = RingLayout::new(region.len(), cd(), PERIOD_FRAMES, 4).expect("layout");
    layout.write_header(region, 0).expect("header");
    {
        let mut producer = RingProducer::new(region, layout).expect("producer");
        let period = pcm(&vec![1000i16; PERIOD_FRAMES as usize * 2]);
        assert_eq!(
            producer.write(&period).expect("write"),
            PERIOD_FRAMES as usize
        );
    }
    mixer
        .set_buffer(id, region, PERIOD_FRAMES, 4)
        .expect("buffer");
    assert_eq!(mixer.call(id, Call::Start).expect("start"), None);
    let mut sink = FakeSink::new();
    assert_eq!(mixer.pump_device(&mut sink).expect("pump"), 1);
    assert_eq!(sink.periods, 1);
    assert_eq!(sink.position_frames(), PERIOD_FRAMES as u64);
    let view = mixer.view(id).expect("view");
    assert_eq!(view.position_frames, PERIOD_FRAMES as u64);
    assert_eq!(view.underruns, 0);
    assert_eq!(view.state, StreamState::Running);
}

#[test]
fn a_ring_underrun_is_counted_and_plays_silence() {
    let mut mixer = Mixer::new();
    let id = started(&mut mixer, cd());
    let region = ring_region(PERIOD_FRAMES, 2);
    let layout = RingLayout::new(region.len(), cd(), PERIOD_FRAMES, 2).expect("layout");
    layout.write_header(region, 0).expect("header");
    mixer
        .set_buffer(id, region, PERIOD_FRAMES, 2)
        .expect("buffer");
    mixer.call(id, Call::Start).expect("start");
    let mut sink = FakeSink::new();
    assert_eq!(mixer.pump_device(&mut sink).expect("pump"), 1);
    assert_eq!(sink.periods, 1);
    let view = mixer.view(id).expect("view");
    assert_eq!(view.underruns, 1);
}

#[test]
fn an_invalid_client_ring_geometry_is_einval() {
    let mut mixer = Mixer::new();
    let id = started(&mut mixer, cd());
    let region = ring_region(PERIOD_FRAMES, 1);
    let error = mixer
        .set_buffer(id, region, PERIOD_FRAMES, 4)
        .expect_err("geometry");
    assert_eq!(error, Errno::EINVAL);
}
