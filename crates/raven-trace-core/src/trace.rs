// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The bounded span registry.

use alloc::collections::{BTreeMap, VecDeque};

use crate::{SpanId, TraceContext, TraceId};

/// Default number of spans one [`Trace`] may hold at once.
pub const DEFAULT_MAX_SPANS: usize = 1024;
/// Default number of records one span may hold.
pub const DEFAULT_MAX_RECORDS: usize = 64;
/// Largest accepted value for `max_spans`.
pub const MAX_SPANS: usize = 4096;
/// Largest accepted value for `max_records`.
pub const MAX_RECORDS: usize = 4096;

/// Bounds for one [`Trace`]. Values are clamped to `1..=MAX_*` when the registry is built.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Limits {
    /// Maximum spans alive at once.
    pub max_spans: usize,
    /// Maximum records one span retains.
    pub max_records: usize,
}

impl Limits {
    /// Create a limit set; values are clamped when the registry is built.
    pub const fn new(max_spans: usize, max_records: usize) -> Self {
        Self {
            max_spans,
            max_records,
        }
    }

    /// Clamp both fields into their supported range.
    pub(crate) const fn clamped(self) -> Self {
        Self {
            max_spans: clamp(self.max_spans, MAX_SPANS),
            max_records: clamp(self.max_records, MAX_RECORDS),
        }
    }
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            max_spans: DEFAULT_MAX_SPANS,
            max_records: DEFAULT_MAX_RECORDS,
        }
    }
}

const fn clamp(value: usize, max: usize) -> usize {
    if value == 0 {
        1
    } else if value > max {
        max
    } else {
        value
    }
}

/// Why a [`Trace`] operation failed.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Error {
    /// The span id is not in the registry.
    NoSuchSpan,
    /// The span is already closed.
    SpanClosed,
    /// The parent is missing or closed, so a child cannot be opened under it.
    ParentInactive,
    /// The registry is full.
    TooManySpans,
    /// The trace id is zero: nothing to trace.
    Disabled,
}

impl Error {
    /// A stable short name for logs and telemetry.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::NoSuchSpan => "no-such-span",
            Self::SpanClosed => "span-closed",
            Self::ParentInactive => "parent-inactive",
            Self::TooManySpans => "too-many-spans",
            Self::Disabled => "disabled",
        }
    }
}

impl core::fmt::Display for Error {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter.write_str(self.as_str())
    }
}

/// Whether a span is open or closed.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum SpanState {
    Active,
    Closed,
}

/// One span and its bounded records.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
struct Span {
    trace_id: TraceId,
    /// Local parent, set by [`Trace::open_child`]; cleared when the span closes.
    parent: Option<SpanId>,
    /// Remote parent, set by [`Trace::adopt`] for cross-process correlation; cleared on close.
    remote_parent: Option<SpanId>,
    sampled: bool,
    state: SpanState,
    records: VecDeque<u64>,
    dropped: usize,
}

impl Span {
    fn new(
        trace_id: TraceId,
        parent: Option<SpanId>,
        remote_parent: Option<SpanId>,
        sampled: bool,
    ) -> Self {
        Self {
            trace_id,
            parent,
            remote_parent,
            sampled,
            state: SpanState::Active,
            records: VecDeque::new(),
            dropped: 0,
        }
    }
}

/// A bounded registry of spans for one process's in-flight traces.
///
/// Spans enter through [`Trace::adopt`] (a new trace root, or a correlated server span) or
/// [`Trace::open_child`] (a local child), and leave through [`Trace::close`]. Closing severs the
/// span's parent links so a closed span holds no reference to its parent.
#[derive(Clone, Debug)]
#[cfg_attr(test, derive(PartialEq, Eq, Hash))]
pub struct Trace {
    spans: BTreeMap<SpanId, Span>,
    next_span_id: u64,
    limits: Limits,
}

impl Default for Trace {
    fn default() -> Self {
        Self::new(Limits::default())
    }
}

impl Trace {
    /// An empty registry with the given (clamped) limits.
    pub fn new(limits: Limits) -> Self {
        Self {
            spans: BTreeMap::new(),
            next_span_id: 1,
            limits: limits.clamped(),
        }
    }

    /// An empty registry with the given capacities (clamped).
    pub fn with_capacity(max_spans: usize, max_records: usize) -> Self {
        Self::new(Limits::new(max_spans, max_records))
    }

    /// The effective limits.
    pub const fn limits(&self) -> Limits {
        self.limits
    }

    /// Adopt an incoming context as a new span.
    ///
    /// This is the correlation point: the caller's `trace_id` and sampling decision become the new
    /// span's, and the caller's `span_id` becomes its *remote* parent (when present). A client
    /// starting a fresh trace calls it with a newly generated id and [`SpanId::NONE`]; a server
    /// receiving a call calls it with the context stamped on the call.
    pub fn adopt(&mut self, context: TraceContext) -> Result<SpanId, Error> {
        if context.trace_id.is_zero() {
            return Err(Error::Disabled);
        }
        let span_id = self.alloc()?;
        let remote_parent = if context.span_id.is_none() {
            None
        } else {
            Some(context.span_id)
        };
        self.spans.insert(
            span_id,
            Span::new(context.trace_id, None, remote_parent, context.sampled),
        );
        Ok(span_id)
    }

    /// Open a child span under an active local `parent`.
    ///
    /// The child inherits the parent's `trace_id` and sampling decision.
    pub fn open_child(&mut self, parent: SpanId) -> Result<SpanId, Error> {
        let Some(parent_span) = self.spans.get(&parent) else {
            return Err(Error::NoSuchSpan);
        };
        if matches!(parent_span.state, SpanState::Closed) {
            return Err(Error::ParentInactive);
        }
        let trace_id = parent_span.trace_id;
        let sampled = parent_span.sampled;
        let span_id = self.alloc()?;
        self.spans
            .insert(span_id, Span::new(trace_id, Some(parent), None, sampled));
        Ok(span_id)
    }

    /// Close a span: the terminal transition. The parent links are severed, so the span no longer
    /// references its parent; it can no longer record or have children opened under it.
    pub fn close(&mut self, span: SpanId) -> Result<(), Error> {
        let Some(s) = self.spans.get_mut(&span) else {
            return Err(Error::NoSuchSpan);
        };
        if matches!(s.state, SpanState::Closed) {
            return Err(Error::SpanClosed);
        }
        s.state = SpanState::Closed;
        s.parent = None;
        s.remote_parent = None;
        Ok(())
    }

    /// Append one record to an active span. Records are bounded: when the list is full the oldest
    /// record is dropped and the span's drop count advances.
    pub fn record(&mut self, span: SpanId, event: u64) -> Result<(), Error> {
        let Some(s) = self.spans.get_mut(&span) else {
            return Err(Error::NoSuchSpan);
        };
        if matches!(s.state, SpanState::Closed) {
            return Err(Error::SpanClosed);
        }
        if s.records.len() >= self.limits.max_records {
            s.records.pop_front();
            s.dropped = s.dropped.saturating_add(1);
        }
        s.records.push_back(event);
        Ok(())
    }

    /// The span's local parent, or `None` for a root span, an adopted span, or a closed span.
    pub fn parent(&self, span: SpanId) -> Option<SpanId> {
        self.spans.get(&span).and_then(|s| s.parent)
    }

    /// The span's remote (cross-process) parent, or `None`.
    pub fn remote_parent(&self, span: SpanId) -> Option<SpanId> {
        self.spans.get(&span).and_then(|s| s.remote_parent)
    }

    /// The span's trace id, or `None` if it is not present.
    pub fn trace_id(&self, span: SpanId) -> Option<TraceId> {
        self.spans.get(&span).map(|s| s.trace_id)
    }

    /// Whether the span is present and open.
    pub fn is_active(&self, span: SpanId) -> bool {
        matches!(
            self.spans.get(&span).map(|s| s.state),
            Some(SpanState::Active)
        )
    }

    /// Whether the span is present and sampled.
    pub fn is_sampled(&self, span: SpanId) -> bool {
        self.spans.get(&span).is_some_and(|s| s.sampled)
    }

    /// Whether the span id is in the registry.
    pub fn contains(&self, span: SpanId) -> bool {
        self.spans.contains_key(&span)
    }

    /// The number of records the span currently holds, or `None` if it is not present.
    pub fn records_len(&self, span: SpanId) -> Option<usize> {
        self.spans.get(&span).map(|s| s.records.len())
    }

    /// The record at `index` (oldest first), or `None` when out of range or the span is missing.
    pub fn record_at(&self, span: SpanId, index: usize) -> Option<u64> {
        self.spans
            .get(&span)
            .and_then(|s| s.records.get(index))
            .copied()
    }

    /// Records dropped by `span` because its list was full, or `None` if it is not present.
    pub fn dropped(&self, span: SpanId) -> Option<usize> {
        self.spans.get(&span).map(|s| s.dropped)
    }

    /// All live span ids, in ascending order.
    pub fn spans(&self) -> impl Iterator<Item = SpanId> + '_ {
        self.spans.keys().copied()
    }

    /// The number of live spans.
    pub fn len(&self) -> usize {
        self.spans.len()
    }

    /// Whether the registry is empty.
    pub fn is_empty(&self) -> bool {
        self.spans.is_empty()
    }

    fn alloc(&mut self) -> Result<SpanId, Error> {
        if self.spans.len() >= self.limits.max_spans {
            return Err(Error::TooManySpans);
        }
        let id = SpanId::from_raw(self.next_span_id);
        self.next_span_id = self.next_span_id.wrapping_add(1);
        if self.next_span_id == 0 {
            self.next_span_id = 1;
        }
        Ok(id)
    }
}

#[cfg(test)]
#[path = "trace_ut.rs"]
mod tests;
