// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host tests for the propagatable trace context.

use crate::{SpanId, TraceContext, TraceId};

#[test]
fn disabled_constant_is_not_traced() {
    let ctx = TraceContext::DISABLED;
    assert!(ctx.is_disabled());
    assert!(ctx.trace_id.is_zero());
    assert!(ctx.span_id.is_none());
    assert!(!ctx.sampled);
    assert!(TraceContext::default().is_disabled());
}

#[test]
fn new_context_keeps_fields() {
    let id = TraceId::from_raw(42);
    let span = SpanId::from_raw(7);
    let ctx = TraceContext::new(id, span, true);
    assert_eq!(ctx.trace_id, id);
    assert_eq!(ctx.span_id, span);
    assert!(ctx.sampled);
    assert!(!ctx.is_disabled());
}
