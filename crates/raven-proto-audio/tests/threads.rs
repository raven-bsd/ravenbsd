// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Two-thread stress test of the PCM ring helpers.
//!
//! The helpers are single-producer/single-consumer. This test gives each side its own OS thread,
//! serialises each individual ring operation with a mutex (the region is one `Vec<u8>`), and
//! checks that every frame arrives once, in order. The memory-ordering contract itself is
//! documented in the README and enforced by the fences in `ring.rs`.

use std::sync::{Arc, Mutex};
use std::thread;

use raven_proto_audio::{Format, RingConsumer, RingLayout, RingProducer, SampleFormat};

const FORMAT: Format = Format {
    rate: 48_000,
    channels: 2,
    sample: SampleFormat::S16Le,
};
const PERIOD: u32 = 256;
const PERIODS: u16 = 4;
const FRAME_BYTES: usize = 4;
const REGION_LEN: usize = 128 + PERIOD as usize * PERIODS as usize * FRAME_BYTES;
const TOTAL_FRAMES: u32 = 8_192;

fn layout() -> RingLayout {
    RingLayout::new(REGION_LEN, FORMAT, PERIOD, PERIODS).expect("layout")
}

fn period(base: u32) -> Vec<u8> {
    let mut bytes = vec![0u8; PERIOD as usize * FRAME_BYTES];
    for (index, frame) in bytes.chunks_exact_mut(FRAME_BYTES).enumerate() {
        frame.copy_from_slice(&base.wrapping_add(index as u32).to_le_bytes());
    }
    bytes
}

#[test]
fn producer_and_consumer_threads_see_every_frame_once() {
    let layout = layout();
    let mut region = vec![0u8; layout.region_len()];
    layout.write_header(&mut region, 3).expect("header");
    let shared = Arc::new(Mutex::new(region));

    let producer_region = Arc::clone(&shared);
    let producer = thread::spawn(move || {
        for base in (0..TOTAL_FRAMES).step_by(PERIOD as usize) {
            let data = period(base);
            loop {
                let written = {
                    let mut guard = producer_region.lock().expect("producer lock");
                    let mut ring = RingProducer::new(&mut guard, layout).expect("producer");
                    ring.write(&data).expect("write")
                };
                if written == PERIOD as usize {
                    break;
                }
                std::hint::spin_loop();
            }
        }
    });

    let consumer_region = Arc::clone(&shared);
    let consumer = thread::spawn(move || {
        let mut buffer = vec![0u8; PERIOD as usize * FRAME_BYTES];
        let mut read = 0u32;
        while read < TOTAL_FRAMES {
            let frames = {
                let mut guard = consumer_region.lock().expect("consumer lock");
                let mut ring = RingConsumer::new(&mut guard, layout).expect("consumer");
                match ring.read_period(&mut buffer) {
                    Ok(frames) => frames,
                    Err(raven_proto_audio::ring::RingError::Underrun) => 0,
                    Err(error) => panic!("unexpected ring error: {error}"),
                }
            };
            if frames == 0 {
                std::hint::spin_loop();
                continue;
            }
            assert_eq!(frames, PERIOD as usize);
            for (index, frame) in buffer.chunks_exact(FRAME_BYTES).enumerate() {
                let expected = read.wrapping_add(index as u32);
                assert_eq!(
                    frame,
                    expected.to_le_bytes(),
                    "frame {}",
                    read + index as u32
                );
            }
            read += frames as u32;
        }
        read
    });

    producer.join().expect("producer thread");
    assert_eq!(consumer.join().expect("consumer thread"), TOTAL_FRAMES);
}
