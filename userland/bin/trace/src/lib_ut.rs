// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host tests for the trace command's drain and formatting.

use raven_trace_core::{RingHeader, Span, SpanKind, SpanRecord, SpanRing};

use crate::{format_tree, kind_name, read_spans};

fn storage(capacity: u32) -> (RingHeader, Vec<SpanRecord>) {
    let header = RingHeader::new(capacity, 4);
    let records: Vec<SpanRecord> = (0..capacity).map(|_| SpanRecord::default()).collect();
    (header, records)
}

fn span(trace_id: u64, span_id: u64, pid: u32, kind: SpanKind) -> Span {
    Span {
        trace_id,
        span_id,
        parent_span_id: 0,
        timestamp_ns: 1000,
        pid,
        kind,
        opcode: 1,
    }
}

#[test]
fn drains_committed_spans() {
    let (header, records) = storage(8);
    let ring = SpanRing::new(&header, &records).unwrap();
    ring.emit(0, 0, &span(0xDEAD_BEEF, 1, 7, SpanKind::Server))
        .unwrap();
    ring.emit(0, 1, &span(0, 2, 7, SpanKind::Server)).unwrap();

    let (spans, gaps) = read_spans(&ring);
    assert_eq!(spans.len(), 2);
    assert_eq!(gaps, 0);
    assert_eq!(spans[0].trace_id, 0xDEAD_BEEF);
    assert_eq!(spans[0].span_id, 1);
    assert_eq!(spans[1].trace_id, 0);
}

#[test]
fn empty_ring_formats_zero_spans() {
    let (header, records) = storage(8);
    let ring = SpanRing::new(&header, &records).unwrap();
    let (spans, gaps) = read_spans(&ring);
    assert!(spans.is_empty());
    assert_eq!(
        format_tree(&spans, gaps),
        "trace: spans=0 gaps=0 traced=0 untraced=0\n"
    );
}

#[test]
fn formats_traced_and_untraced() {
    let (header, records) = storage(8);
    let ring = SpanRing::new(&header, &records).unwrap();
    ring.emit(0, 0, &span(0xDEAD_BEEF, 1, 7, SpanKind::Server))
        .unwrap();
    ring.emit(0, 1, &span(0, 2, 7, SpanKind::Server)).unwrap();

    let (spans, gaps) = read_spans(&ring);
    let text = format_tree(&spans, gaps);
    assert!(text.starts_with("trace: spans=2 gaps=0 traced=1 untraced=1\n"));
    assert!(
        text.contains("trace: id=0xdeadbeef span=0x1 parent=0x0 pid=7 ts=1000 kind=server op=1")
    );
    assert!(text.contains("trace: id=untraced span=0x2 parent=0x0 pid=7 ts=1000 kind=server op=1"));
}

#[test]
fn kind_names_are_stable() {
    assert_eq!(kind_name(SpanKind::Client), "client");
    assert_eq!(kind_name(SpanKind::Server), "server");
    assert_eq!(kind_name(SpanKind::Kernel), "kernel");
    assert_eq!(kind_name(SpanKind::Io), "io");
}
