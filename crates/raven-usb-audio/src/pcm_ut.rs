// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Assertions for the PCM source seam and the audiod ring adapter.

use super::*;
use raven_proto_audio::format::{Format, SampleFormat};
use raven_proto_audio::ring::{RingConsumer, RingLayout, RingProducer};

#[test]
fn a_slice_source_reads_whole_frames() {
    let data = [0u8; 16];
    let mut source = SliceSource::new(&data, 4).expect("source");
    assert_eq!(source.frame_bytes(), 4);
    assert_eq!(source.available_frames(), 4);
    assert_eq!(source.remaining(), 16);

    let mut out = [0u8; 12];
    assert_eq!(source.read(&mut out), 12);
    assert_eq!(source.position(), 12);
    assert_eq!(source.available_frames(), 1);
    assert_eq!(source.read(&mut out), 4);
    assert_eq!(source.remaining(), 0);
    assert_eq!(source.read(&mut out), 0);
}

#[test]
fn a_slice_source_rejects_bad_geometry() {
    assert!(SliceSource::new(&[0u8; 10], 4).is_none());
    assert!(SliceSource::new(&[0u8; 8], 0).is_none());
    assert!(SliceSource::new(&[0u8; 8], 4).is_some());
}

#[test]
fn a_slice_source_returns_the_data() {
    let data = [1u8, 2, 3, 4, 5, 6, 7, 8];
    let mut source = SliceSource::new(&data, 4).expect("source");
    let mut out = [0u8; 4];
    assert_eq!(source.read(&mut out), 4);
    assert_eq!(out, [1, 2, 3, 4]);
    assert_eq!(source.read(&mut out), 4);
    assert_eq!(out, [5, 6, 7, 8]);
}

#[test]
fn the_audiod_ring_consumer_implements_the_source() {
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
        let pcm: Vec<u8> = (0..64u8)
            .flat_map(|frame| [frame, frame, frame, frame])
            .collect();
        assert_eq!(producer.write(&pcm), Ok(64), "write returns frames");
    }
    let mut consumer = RingConsumer::new(&mut region, layout).expect("consumer");
    assert_eq!(PcmSource::frame_bytes(&consumer), 4);
    assert_eq!(PcmSource::available_frames(&consumer), 64);
    let mut out = [0u8; 8];
    assert_eq!(PcmSource::read(&mut consumer, &mut out), 8);
    assert_eq!(out, [0, 0, 0, 0, 1, 1, 1, 1]);
    assert_eq!(PcmSource::available_frames(&consumer), 62);
}
