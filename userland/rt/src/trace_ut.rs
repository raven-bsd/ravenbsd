// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host tests for the safe sink handle.

use super::*;

fn span(trace_id: u64) -> Span {
    Span {
        trace_id,
        span_id: 1,
        parent_span_id: 0,
        timestamp_ns: 2,
        pid: 0,
        kind: raven_trace_core::SpanKind::Server,
        opcode: 7,
    }
}

fn ring() -> SpanRing<'static> {
    let header = Box::leak(Box::new(RingHeader::new(CAPACITY, PRODUCERS)));
    let records: Vec<SpanRecord> = (0..CAPACITY).map(|_| SpanRecord::default()).collect();
    let records: &'static [SpanRecord] = Box::leak(records.into_boxed_slice());
    SpanRing::new(header, records).expect("ring")
}

#[test]
fn sink_pages_cover_the_ring() {
    const { assert!(SINK_PAGES * PAGE >= SINK_BYTES) };
    const { assert!((SINK_PAGES - 1) * PAGE < SINK_BYTES) };
}

#[test]
fn producer_index_is_bounded() {
    for producer in 0..PRODUCERS {
        assert!(producer_valid(producer));
    }
    assert!(!producer_valid(PRODUCERS));
    assert!(!producer_valid(u32::MAX));
}

#[test]
fn disabled_sink_reports_disabled_and_emits_nothing() {
    let mut sink = Sink::disabled(0);
    assert!(!sink.enabled());
    assert!(!sink.emit(span(1)));
}

#[test]
fn zero_trace_id_is_recorded_as_untraced() {
    let mut sink = Sink {
        ring: Some(ring()),
        producer: 0,
        sequence: 0,
    };
    assert!(sink.enabled());
    assert!(sink.emit(span(0)));
    let mut cursor = 0;
    let record = sink
        .ring
        .as_ref()
        .expect("ring")
        .read(&mut cursor)
        .expect("span");
    assert_eq!(record.trace_id, 0);
}

#[test]
fn traced_span_is_recorded() {
    let mut sink = Sink {
        ring: Some(ring()),
        producer: 0,
        sequence: 0,
    };
    assert!(sink.emit(span(0xDEAD_BEEF)));
    let mut cursor = 0;
    let record = sink
        .ring
        .as_ref()
        .expect("ring")
        .read(&mut cursor)
        .expect("span");
    assert_eq!(record.trace_id, 0xDEAD_BEEF);
    assert_eq!(record.opcode, 7);
}
