// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host tests for the tracing facade and its disabled fast path.

use crate::{Limits, SpanId, TraceContext, TraceId, Tracer};

#[test]
fn disabled_tracer_allocates_nothing() {
    let mut tracer = Tracer::with_enabled(Limits::new(16, 8), false);
    let ctx = TraceContext::new(TraceId::from_raw(1), SpanId::NONE, true);

    assert_eq!(tracer.start_span(ctx), None);
    assert_eq!(tracer.child(SpanId::from_raw(1)), None);
    assert!(!tracer.close(SpanId::from_raw(1)));
    assert!(!tracer.record(SpanId::from_raw(1), 42));
    assert_eq!(tracer.len(), 0, "disabled tracer must not create spans");
    assert!(tracer.is_empty());
}

#[test]
fn disabled_context_takes_the_fast_path_even_when_enabled() {
    let mut tracer = Tracer::with_capacity(16, 8);
    assert_eq!(tracer.start_span(TraceContext::DISABLED), None);
    assert_eq!(tracer.len(), 0);
}

#[test]
fn enabled_tracer_correlates_and_records() {
    let mut tracer = Tracer::with_capacity(16, 8);
    let client = tracer
        .start_span(TraceContext::new(TraceId::from_raw(5), SpanId::NONE, true))
        .unwrap();

    // The server adopts the caller's context, correlating by trace_id.
    let server = tracer
        .start_span(TraceContext::new(TraceId::from_raw(5), client, true))
        .unwrap();

    assert_eq!(tracer.trace().trace_id(server), Some(TraceId::from_raw(5)));
    assert_eq!(tracer.trace().remote_parent(server), Some(client));
    assert_eq!(tracer.trace().parent(server), None);

    assert!(tracer.record(server, 1));
    assert!(tracer.record(server, 2));
    assert_eq!(tracer.trace().records_len(server), Some(2));
    assert_eq!(tracer.len(), 2);
}

#[test]
fn enabling_flip_toggles_the_fast_path() {
    let mut tracer = Tracer::with_enabled(Limits::new(16, 8), false);
    let ctx = TraceContext::new(TraceId::from_raw(1), SpanId::NONE, false);
    assert_eq!(tracer.start_span(ctx), None);

    tracer.set_enabled(true);
    assert!(tracer.is_enabled());
    let span = tracer.start_span(ctx).unwrap();
    assert_eq!(tracer.len(), 1);
    assert!(tracer.record(span, 9));

    tracer.set_enabled(false);
    assert!(!tracer.is_enabled());
    assert!(!tracer.record(span, 10));
    assert_eq!(tracer.trace().records_len(span), Some(1));
}

#[test]
fn child_and_close_go_through_the_tracer() {
    let mut tracer = Tracer::with_capacity(16, 8);
    let root = tracer
        .start_span(TraceContext::new(TraceId::from_raw(1), SpanId::NONE, false))
        .unwrap();
    let child = tracer.child(root).unwrap();
    assert_eq!(tracer.trace().parent(child), Some(root));

    assert!(tracer.close(child));
    assert_eq!(tracer.trace().parent(child), None, "no parent after close");
    assert!(!tracer.close(child));
}
