// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The trace state that crosses a process boundary.

use crate::{SpanId, TraceId};

/// The trace state a thread carries and that propagates through a call.
///
/// A client that starts a new trace builds one from a freshly generated [`TraceId`] and a
/// [`Sampling`](crate::Sampling) decision; a server that receives a call adopts the caller's
/// context, correlating its spans with the caller's by `trace_id`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct TraceContext {
    /// The trace this work belongs to; zero means "not traced".
    pub trace_id: TraceId,
    /// The caller's current span, or [`SpanId::NONE`] when the caller has no span.
    pub span_id: SpanId,
    /// The sampling decision for the trace.
    pub sampled: bool,
}

impl TraceContext {
    /// The "not tracing" context: no id, no span, not sampled.
    pub const DISABLED: Self = Self {
        trace_id: TraceId::ZERO,
        span_id: SpanId::NONE,
        sampled: false,
    };

    /// Build a context from its three fields.
    pub const fn new(trace_id: TraceId, span_id: SpanId, sampled: bool) -> Self {
        Self {
            trace_id,
            span_id,
            sampled,
        }
    }

    /// Whether tracing is disabled for this context (the `trace_id` is zero).
    pub const fn is_disabled(self) -> bool {
        self.trace_id.is_zero()
    }
}

impl Default for TraceContext {
    fn default() -> Self {
        Self::DISABLED
    }
}

#[cfg(test)]
#[path = "context_ut.rs"]
mod tests;
