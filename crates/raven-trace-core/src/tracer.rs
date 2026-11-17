// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The tracing facade with the disabled fast path.

use crate::{Limits, SpanId, Trace, TraceContext};

/// A [`Trace`] wrapped with a global enable flag.
///
/// When tracing is disabled — either the flag is off or the incoming context has a zero
/// [`TraceContext::trace_id`] — every operation returns `None`/`false` without touching the
/// registry, so the disabled path does not allocate. This is the pure form of `R-OBS-11`'s
/// zero-cost-when-disabled branch.
#[derive(Clone, Debug)]
#[cfg_attr(test, derive(PartialEq, Eq, Hash))]
pub struct Tracer {
    enabled: bool,
    trace: Trace,
}

impl Default for Tracer {
    fn default() -> Self {
        Self::new(Limits::default())
    }
}

impl Tracer {
    /// An enabled tracer with the given (clamped) limits.
    pub fn new(limits: Limits) -> Self {
        Self::with_enabled(limits, true)
    }

    /// An enabled tracer with the given capacities (clamped).
    pub fn with_capacity(max_spans: usize, max_records: usize) -> Self {
        Self::new(Limits::new(max_spans, max_records))
    }

    /// A tracer whose enable flag is `enabled`.
    pub fn with_enabled(limits: Limits, enabled: bool) -> Self {
        Self {
            enabled,
            trace: Trace::new(limits),
        }
    }

    /// Whether tracing is enabled.
    pub fn is_enabled(&self) -> bool {
        self.enabled
    }

    /// Set the enable flag.
    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
    }

    /// Start a span from an incoming context; the correlation entry point.
    ///
    /// Returns `None` — without allocating — when tracing is disabled or the context is disabled,
    /// or when the registry is full.
    pub fn start_span(&mut self, context: TraceContext) -> Option<SpanId> {
        if !self.enabled || context.trace_id.is_zero() {
            return None;
        }
        self.trace.adopt(context).ok()
    }

    /// Open a local child under `parent`.
    pub fn child(&mut self, parent: SpanId) -> Option<SpanId> {
        if !self.enabled {
            return None;
        }
        self.trace.open_child(parent).ok()
    }

    /// Close `span`; `true` on success.
    pub fn close(&mut self, span: SpanId) -> bool {
        self.enabled && self.trace.close(span).is_ok()
    }

    /// Append one record to `span`; `true` on success.
    pub fn record(&mut self, span: SpanId, event: u64) -> bool {
        self.enabled && self.trace.record(span, event).is_ok()
    }

    /// The underlying registry (read-only view of spans, parents and records).
    pub fn trace(&self) -> &Trace {
        &self.trace
    }

    /// The effective limits.
    pub const fn limits(&self) -> Limits {
        self.trace.limits()
    }

    /// The number of live spans.
    pub fn len(&self) -> usize {
        self.trace.len()
    }

    /// Whether the registry is empty.
    pub fn is_empty(&self) -> bool {
        self.trace.is_empty()
    }
}

#[cfg(test)]
#[path = "tracer_ut.rs"]
mod tests;
