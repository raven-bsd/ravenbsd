// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host tests for the cross-process span sink.

use core::sync::atomic::Ordering;

use crate::sink::{
    RingError, RingHeader, Span, SpanKind, SpanRecord, SpanRing, TraceReader, TraceSink,
};

fn storage(capacity: u32) -> (RingHeader, Vec<SpanRecord>) {
    let header = RingHeader::new(capacity, 4);
    let records: Vec<SpanRecord> = (0..capacity).map(|_| SpanRecord::default()).collect();
    (header, records)
}

fn span(trace_id: u64) -> Span {
    Span {
        trace_id,
        span_id: trace_id.wrapping_add(1),
        parent_span_id: 0,
        timestamp_ns: trace_id,
        pid: 0,
        kind: SpanKind::Client,
        opcode: 0,
    }
}

#[test]
fn header_validation_rejects_bad_input() {
    let header = RingHeader::new(8, 4);
    let records: Vec<SpanRecord> = (0..8).map(|_| SpanRecord::default()).collect();
    assert!(SpanRing::new(&header, &records).is_ok());

    // Not a power of two.
    let bad_capacity = RingHeader::new(3, 4);
    assert!(matches!(
        SpanRing::new(&bad_capacity, &records),
        Err(RingError::BadCapacity)
    ));

    // Zero capacity.
    let zero = RingHeader::new(0, 4);
    assert!(matches!(
        SpanRing::new(&zero, &records),
        Err(RingError::BadCapacity)
    ));

    // Too few records.
    let short: Vec<SpanRecord> = (0..4).map(|_| SpanRecord::default()).collect();
    assert!(matches!(
        SpanRing::new(&header, &short),
        Err(RingError::BadCapacity)
    ));
}

#[test]
fn single_producer_round_trips_in_order() {
    let (header, records) = storage(8);
    let ring = SpanRing::new(&header, &records).unwrap();
    for sequence in 0..8u64 {
        ring.emit(0, sequence, &span(sequence * 10)).unwrap();
    }
    let mut cursor = 0u64;
    for sequence in 0..8u64 {
        let read = ring.read(&mut cursor).unwrap();
        assert_eq!(read.trace_id, sequence * 10);
    }
    assert_eq!(ring.read(&mut cursor), Err(RingError::Empty));
}

#[test]
fn wrap_reuses_slots_over_capacity() {
    let (header, records) = storage(4);
    let ring = SpanRing::new(&header, &records).unwrap();
    let mut cursor = 0u64;
    for sequence in 0..20u64 {
        ring.emit(0, sequence, &span(sequence)).unwrap();
        assert_eq!(ring.read(&mut cursor).unwrap().trace_id, sequence);
    }
    assert_eq!(ring.read(&mut cursor), Err(RingError::Empty));
}

#[test]
fn gap_is_detected_when_reader_falls_behind() {
    let (header, records) = storage(2);
    let ring = SpanRing::new(&header, &records).unwrap();
    ring.emit(0, 0, &span(1)).unwrap();
    ring.emit(0, 1, &span(2)).unwrap();
    ring.emit(0, 2, &span(3)).unwrap(); // overwrites span(1)

    let mut cursor = 0u64;
    assert_eq!(ring.read(&mut cursor), Err(RingError::Gap { missed: 1 }));
    assert_eq!(cursor, 1);
    assert_eq!(ring.read(&mut cursor).unwrap().trace_id, 2);
    assert_eq!(ring.read(&mut cursor).unwrap().trace_id, 3);
    assert_eq!(ring.read(&mut cursor), Err(RingError::Empty));
}

#[test]
fn torn_record_is_rejected() {
    let (header, records) = storage(2);
    let ring = SpanRing::new(&header, &records).unwrap();
    // Simulate a malformed ring: the commit cursor advanced past a slot whose generation is odd
    // (a producer reserved the slot but never finished writing it).
    records[0].generation.store(1, Ordering::Release);
    header.commit_cursor.store(1, Ordering::Release);
    let mut cursor = 0u64;
    assert_eq!(ring.read(&mut cursor), Err(RingError::Torn));
}

#[test]
fn bad_producer_is_rejected() {
    let (header, records) = storage(4);
    let ring = SpanRing::new(&header, &records).unwrap();
    assert_eq!(ring.emit(4, 0, &span(1)), Err(RingError::BadProducer));
    assert_eq!(ring.emit(99, 0, &span(1)), Err(RingError::BadProducer));
}

#[test]
fn interleaved_reserve_write_publish_commits_in_order() {
    let (header, records) = storage(8);
    let ring = SpanRing::new(&header, &records).unwrap();
    // Four producers reserve in slot order, write out of order, publish in slot order.
    let slots = [
        ring.reserve(),
        ring.reserve(),
        ring.reserve(),
        ring.reserve(),
    ];
    ring.write(slots[3], 0, &span(30));
    ring.write(slots[1], 0, &span(10));
    ring.write(slots[0], 0, &span(0));
    ring.write(slots[2], 0, &span(20));
    ring.publish(slots[0]);
    ring.publish(slots[1]);
    ring.publish(slots[2]);
    ring.publish(slots[3]);

    let mut cursor = 0u64;
    assert_eq!(ring.read(&mut cursor).unwrap().trace_id, 0);
    assert_eq!(ring.read(&mut cursor).unwrap().trace_id, 10);
    assert_eq!(ring.read(&mut cursor).unwrap().trace_id, 20);
    assert_eq!(ring.read(&mut cursor).unwrap().trace_id, 30);
    assert_eq!(ring.read(&mut cursor), Err(RingError::Empty));
}

#[test]
fn trace_sink_disabled_writes_nothing() {
    let (header, records) = storage(4);
    let ring = SpanRing::new(&header, &records).unwrap();
    let mut sink = TraceSink::new(ring, 1);
    sink.set_enabled(false);
    assert!(!sink.is_enabled());
    // Disabled emit writes nothing.
    sink.emit(&span(1)).unwrap();
    assert_eq!(ring.committed(), 0);

    sink.set_enabled(true);
    sink.emit(&span(2)).unwrap();
    assert_eq!(ring.committed(), 1);
}

#[test]
fn trace_reader_cursor_advances() {
    let (header, records) = storage(4);
    let ring = SpanRing::new(&header, &records).unwrap();
    let mut sink = TraceSink::new(ring, 0);
    sink.emit(&span(7)).unwrap();
    sink.emit(&span(8)).unwrap();

    let mut reader = TraceReader::new(ring);
    assert_eq!(reader.cursor(), 0);
    assert_eq!(reader.read().unwrap().trace_id, 7);
    assert_eq!(reader.cursor(), 1);
    assert_eq!(reader.read().unwrap().trace_id, 8);
    assert_eq!(reader.cursor(), 2);
    assert_eq!(reader.read(), Err(RingError::Empty));
}

#[test]
fn tracer_emit_sink_writes_only_when_enabled() {
    use crate::Tracer;

    let (header, records) = storage(4);
    let ring = SpanRing::new(&header, &records).unwrap();
    let mut sink = TraceSink::new(ring, 0);

    let mut tracer = Tracer::default();
    tracer.set_enabled(false);
    assert!(!tracer.emit_sink(&mut sink, span(1)));
    assert_eq!(ring.committed(), 0);

    tracer.set_enabled(true);
    assert!(tracer.emit_sink(&mut sink, span(2)));
    assert_eq!(ring.committed(), 1);
}

// --- deterministic randomized model ------------------------------------------

struct XorShift(u64);

impl XorShift {
    const fn new(seed: u64) -> Self {
        Self(seed)
    }

    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }
}

#[test]
fn deterministic_randomized_model_preserves_descriptors() {
    const CAPACITY: u32 = 8;
    const PRODUCERS: u32 = 4;

    let header = RingHeader::new(CAPACITY, PRODUCERS);
    let records: Vec<SpanRecord> = (0..CAPACITY).map(|_| SpanRecord::default()).collect();
    let ring = SpanRing::new(&header, &records).unwrap();

    let mut rng = XorShift::new(0x5256_4E54_5249_4E47);
    let mut sequence = [0u64; PRODUCERS as usize];
    let mut emitted = 0u64;
    let mut read_total = 0u64;
    let mut gaps = 0u64;
    let mut cursor = 0u64;
    let mut last_per_producer = [None::<u64>; PRODUCERS as usize];

    for _ in 0..200_000 {
        if rng.next().is_multiple_of(3) {
            match ring.read(&mut cursor) {
                Ok(read) => {
                    let producer = (read.trace_id >> 48) as usize;
                    let seq = read.trace_id & ((1u64 << 48) - 1);
                    if let Some(last) = last_per_producer[producer] {
                        assert!(
                            seq > last,
                            "per-producer order: producer={producer} seq={seq} last={last} trace={read:?}"
                        );
                    }
                    last_per_producer[producer] = Some(seq);
                    read_total += 1;
                }
                Err(RingError::Empty) => {}
                Err(RingError::Gap { missed }) => gaps += missed,
                Err(error) => panic!("unexpected error: {error:?}"),
            }
        } else {
            let producer = (rng.next() % u64::from(PRODUCERS)) as u32;
            let seq = sequence[producer as usize];
            sequence[producer as usize] += 1;
            // Encode producer (high 16 bits) + sequence (low 48 bits) for verification.
            let trace_id = (u64::from(producer) << 48) | seq;
            ring.emit(producer, seq, &span(trace_id)).unwrap();
            emitted += 1;
        }
    }

    loop {
        match ring.read(&mut cursor) {
            Ok(read) => {
                let producer = (read.trace_id >> 48) as usize;
                let seq = read.trace_id & ((1u64 << 48) - 1);
                if let Some(last) = last_per_producer[producer] {
                    assert!(seq > last, "per-producer order");
                }
                last_per_producer[producer] = Some(seq);
                read_total += 1;
            }
            Err(RingError::Empty) => break,
            Err(RingError::Gap { missed }) => gaps += missed,
            Err(error) => panic!("unexpected: {error:?}"),
        }
    }

    // No descriptor is lost (read + gap accounts for every emit) and per-producer order holds.
    assert_eq!(read_total + gaps, emitted);
}

// --- multi-producer volume test ----------------------------------------------

#[test]
fn four_producers_hundred_k_records_each() {
    // Four producers, 100,000 records each, interleaved deterministically with a single reader.
    // Exercises wrap, gap detection and per-producer monotonic sequence at volume, with no lost or
    // duplicated record (read + gap accounts for every emit).
    const CAPACITY: u32 = 1024;
    const PRODUCERS: u32 = 4;
    const PER_PRODUCER: u64 = 100_000;

    let header = RingHeader::new(CAPACITY, PRODUCERS);
    let records: Vec<SpanRecord> = (0..CAPACITY).map(|_| SpanRecord::default()).collect();
    let ring = SpanRing::new(&header, &records).unwrap();

    let mut sequence = [0u64; PRODUCERS as usize];
    let mut emitted = 0u64;
    let mut read_total = 0u64;
    let mut gaps = 0u64;
    let mut cursor = 0u64;
    let mut last_per_producer = [None::<u64>; PRODUCERS as usize];
    let expected = u64::from(PRODUCERS) * PER_PRODUCER;

    let mut done_emitting = false;
    while !done_emitting || read_total + gaps < expected {
        // Each producer emits one record per pass until it has emitted PER_PRODUCER.
        done_emitting = true;
        for producer in 0..PRODUCERS {
            if sequence[producer as usize] < PER_PRODUCER {
                let seq = sequence[producer as usize];
                sequence[producer as usize] += 1;
                let trace_id = (u64::from(producer) << 48) | seq;
                ring.emit(producer, seq, &span(trace_id)).unwrap();
                emitted += 1;
                done_emitting = false;
            }
        }
        // Drain a bounded chunk so the ring wraps and occasionally reports a gap.
        for _ in 0..16 {
            match ring.read(&mut cursor) {
                Ok(read) => {
                    let producer = (read.trace_id >> 48) as usize;
                    let seq = read.trace_id & ((1u64 << 48) - 1);
                    if let Some(last) = last_per_producer[producer] {
                        assert!(
                            seq > last,
                            "per-producer order: producer={producer} seq={seq} last={last}"
                        );
                    }
                    last_per_producer[producer] = Some(seq);
                    read_total += 1;
                }
                Err(RingError::Gap { missed }) => gaps += missed,
                Err(RingError::Empty) => break,
                Err(error) => panic!("unexpected: {error:?}"),
            }
        }
    }
    assert_eq!(emitted, expected);
    assert_eq!(read_total + gaps, expected);
}
