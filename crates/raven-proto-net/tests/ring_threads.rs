// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Two-thread stress tests of the ring helpers.
//!
//! The helpers are single-producer/single-consumer. These tests give each side its own OS thread,
//! serialise each individual ring operation with a mutex (the region is one `Vec<u8>`), and check
//! that every descriptor is delivered exactly once, in order. They exercise the index arithmetic
//! and wrap-around under real interleaving; the memory-ordering contract itself is documented in
//! the README and enforced by the fences in `ring.rs`.

use std::sync::{Arc, Mutex};
use std::thread;

use raven_proto_net::{
    BUFFER_SIZE, DESC_FIRST, Desc, Ring, RingConsumer, RingLayout, RingProducer,
};

const MESSAGES: u32 = 10_000;

#[test]
fn producer_and_consumer_threads_see_every_descriptor_once() {
    let layout = RingLayout::new(2048 + 8 * BUFFER_SIZE, 16, 16).expect("layout");
    let mut region = vec![0u8; layout.region_len()];
    layout.write_header(&mut region, 1, 2).expect("header");
    let shared = Arc::new(Mutex::new(region));

    let producer_region = Arc::clone(&shared);
    let producer = thread::spawn(move || {
        for index in 0..MESSAGES {
            loop {
                let pushed = {
                    let mut guard = producer_region.lock().expect("producer lock");
                    let mut ring =
                        RingProducer::new(&mut guard, layout, Ring::Tx).expect("producer");
                    ring.push(Desc::new(u64::from(index), index, DESC_FIRST))
                        .is_ok()
                };
                if pushed {
                    break;
                }
                std::hint::spin_loop();
            }
        }
    });

    let consumer_region = Arc::clone(&shared);
    let consumer = thread::spawn(move || {
        let mut seen = 0u32;
        while seen < MESSAGES {
            let popped = {
                let mut guard = consumer_region.lock().expect("consumer lock");
                let mut ring = RingConsumer::new(&mut guard, layout, Ring::Tx).expect("consumer");
                ring.pop()
            };
            match popped {
                Some(desc) => {
                    assert_eq!(desc.addr, u64::from(seen), "out of order at {seen}");
                    assert_eq!(desc.len, seen);
                    assert!(desc.is_first());
                    seen += 1;
                }
                None => std::hint::spin_loop(),
            }
        }
        seen
    });

    producer.join().expect("producer thread");
    assert_eq!(consumer.join().expect("consumer thread"), MESSAGES);
}

/// The same check with pseudo-random bursts on both sides, so producer and consumer do not stay
/// in lockstep.
#[test]
fn randomised_bursts_deliver_every_descriptor_once() {
    const COUNT: u32 = 4_000;

    let layout = RingLayout::new(2048 + 4 * BUFFER_SIZE, 8, 8).expect("layout");
    let mut region = vec![0u8; layout.region_len()];
    layout.write_header(&mut region, 7, 8).expect("header");
    let shared = Arc::new(Mutex::new(region));

    let producer_region = Arc::clone(&shared);
    let producer = thread::spawn(move || {
        let mut state = 0x0bad_c0de_1234_5678u64;
        let mut sent = 0u32;
        while sent < COUNT {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            let batch = 1 + (state % 4) as u32;
            {
                let mut guard = producer_region.lock().expect("producer lock");
                let mut ring = RingProducer::new(&mut guard, layout, Ring::Tx).expect("producer");
                for _ in 0..batch {
                    if sent == COUNT
                        || ring
                            .push(Desc::new(u64::from(sent), sent, DESC_FIRST))
                            .is_err()
                    {
                        break;
                    }
                    sent += 1;
                }
            }
            std::hint::spin_loop();
        }
    });

    let consumer_region = Arc::clone(&shared);
    let consumer = thread::spawn(move || {
        let mut state = 0xfeed_face_dead_beefu64;
        let mut seen = 0u32;
        while seen < COUNT {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            let batch = 1 + (state % 4) as u32;
            {
                let mut guard = consumer_region.lock().expect("consumer lock");
                let mut ring = RingConsumer::new(&mut guard, layout, Ring::Tx).expect("consumer");
                for _ in 0..batch {
                    match ring.pop() {
                        Some(desc) => {
                            assert_eq!(desc.len, seen, "out of order at {seen}");
                            seen += 1;
                        }
                        None => break,
                    }
                }
            }
            std::hint::spin_loop();
        }
        seen
    });

    producer.join().expect("producer thread");
    assert_eq!(consumer.join().expect("consumer thread"), COUNT);
}
