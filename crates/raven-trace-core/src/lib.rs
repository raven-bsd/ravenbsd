// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Pure trace id, span and correlation model for the native IPC path.
//!
//! This crate owns the policy that later tickets stamp onto the scheme/channel header:
//! non-zero trace/span ids, parent/child spans, a deterministic sampling decision, bounded span
//! records with a no-allocation fast path when tracing is disabled, and the client/server
//! correlation used by a call/reply boundary. It has no kernel dependency, no wire format and no
//! syscall or ABI constants; Q3 supplies the header field and the kernel integration.

#![no_std]
#![forbid(unsafe_code)]

extern crate alloc;

use alloc::vec::Vec;

/// A non-zero trace id shared by every span in one logical operation.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct TraceId(u64);

impl TraceId {
    /// Construct a trace id, rejecting zero.
    #[must_use]
    pub const fn new(raw: u64) -> Option<Self> {
        if raw == 0 { None } else { Some(Self(raw)) }
    }

    /// The raw 64-bit value.
    #[must_use]
    pub const fn raw(self) -> u64 {
        self.0
    }
}

/// A non-zero span id unique inside its trace.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct SpanId(u64);

impl SpanId {
    /// Construct a span id, rejecting zero.
    #[must_use]
    pub const fn new(raw: u64) -> Option<Self> {
        if raw == 0 { None } else { Some(Self(raw)) }
    }

    /// The raw 64-bit value.
    #[must_use]
    pub const fn raw(self) -> u64 {
        self.0
    }
}

/// A deterministic id source. The kernel supplies the seed from its entropy pool.
#[derive(Clone, Copy, Debug)]
pub struct IdGenerator {
    state: u64,
}

impl IdGenerator {
    /// Create a generator from `seed`; every seed is valid.
    #[must_use]
    pub const fn new(seed: u64) -> Self {
        Self { state: seed }
    }

    fn next_u64(&mut self) -> u64 {
        self.state = self.state.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut value = self.state;
        value = (value ^ (value >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        value = (value ^ (value >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        value ^ (value >> 31)
    }

    /// Produce the next non-zero trace id.
    pub fn next_trace(&mut self) -> TraceId {
        loop {
            if let Some(id) = TraceId::new(self.next_u64()) {
                return id;
            }
        }
    }

    /// Produce the next non-zero span id.
    pub fn next_span(&mut self) -> SpanId {
        loop {
            if let Some(id) = SpanId::new(self.next_u64()) {
                return id;
            }
        }
    }
}

/// A deterministic per-mille sampler.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Sampler {
    rate_per_mille: u32,
}

impl Sampler {
    /// Keep `rate_per_mille` of every thousand traces (0 = none, 1000 = all).
    #[must_use]
    pub const fn new(rate_per_mille: u32) -> Self {
        Self {
            rate_per_mille: if rate_per_mille > 1000 {
                1000
            } else {
                rate_per_mille
            },
        }
    }

    /// Keep every trace.
    #[must_use]
    pub const fn always() -> Self {
        Self::new(1000)
    }

    /// Keep no trace.
    #[must_use]
    pub const fn never() -> Self {
        Self::new(0)
    }

    /// Configured rate in per-mille.
    #[must_use]
    pub const fn rate_per_mille(self) -> u32 {
        self.rate_per_mille
    }

    /// Decide one trace deterministically. The same trace id always gets the same decision.
    #[must_use]
    pub fn sample(self, trace: TraceId) -> bool {
        if self.rate_per_mille == 0 {
            return false;
        }
        if self.rate_per_mille >= 1000 {
            return true;
        }
        let mut value = trace.raw().wrapping_add(0x9e37_79b9_7f4a_7c15);
        value = (value ^ (value >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        value = (value ^ (value >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        value ^= value >> 31;
        (value % 1000) < u64::from(self.rate_per_mille)
    }
}

/// A propagated span context: the trace, its current span and the optional parent span.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SpanContext {
    trace: TraceId,
    span: SpanId,
    parent: Option<SpanId>,
    sampled: bool,
}

impl SpanContext {
    /// Start a root span for a new trace.
    #[must_use]
    pub fn root(trace: TraceId, span: SpanId, sampled: bool) -> Self {
        Self {
            trace,
            span,
            parent: None,
            sampled,
        }
    }

    /// Start a child span with the same trace and sampling decision.
    #[must_use]
    pub fn child(&self, span: SpanId, sampled: bool) -> Self {
        Self {
            trace: self.trace,
            span,
            parent: Some(self.span),
            sampled: sampled && self.sampled,
        }
    }

    /// The trace id.
    #[must_use]
    pub const fn trace(self) -> TraceId {
        self.trace
    }

    /// The span id.
    #[must_use]
    pub const fn span(self) -> SpanId {
        self.span
    }

    /// The parent span, if this is not a root.
    #[must_use]
    pub const fn parent(self) -> Option<SpanId> {
        self.parent
    }

    /// Whether the trace is sampled.
    #[must_use]
    pub const fn sampled(self) -> bool {
        self.sampled
    }
}

/// The client and server spans of one call, sharing one trace id.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CallContext {
    client: SpanContext,
    server: SpanContext,
}

impl CallContext {
    /// Correlate a client span with a new server span.
    ///
    /// The server span is a child of the client span, so the trace and sampling decision are
    /// preserved. A reply is recorded on the client span; the caller uses [`Self::reply`].
    #[must_use]
    pub fn new(client: SpanContext, ids: &mut IdGenerator) -> Self {
        let server = client.child(ids.next_span(), client.sampled);
        Self { client, server }
    }

    /// The client span.
    #[must_use]
    pub const fn client(self) -> SpanContext {
        self.client
    }

    /// The server span.
    #[must_use]
    pub const fn server(self) -> SpanContext {
        self.server
    }

    /// The span a reply is recorded on.
    #[must_use]
    pub const fn reply(self) -> SpanContext {
        self.client
    }
}

/// Why a span operation was rejected.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TraceError {
    /// The handle does not name a record in this tracer.
    UnknownSpan,
    /// The span was already closed.
    AlreadyClosed,
    /// A child cannot be started under a closed parent.
    ParentClosed,
}

/// One bounded span record. A record stays allocated after close so a report can read it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SpanRecord {
    context: SpanContext,
    name: u32,
    start_ns: u64,
    end_ns: Option<u64>,
    status: i32,
}

impl SpanRecord {
    /// The span context.
    #[must_use]
    pub const fn context(&self) -> SpanContext {
        self.context
    }

    /// The caller's opaque span name id.
    #[must_use]
    pub const fn name(&self) -> u32 {
        self.name
    }

    /// Start timestamp supplied by the caller.
    #[must_use]
    pub const fn start_ns(&self) -> u64 {
        self.start_ns
    }

    /// End timestamp, or `None` while open.
    #[must_use]
    pub const fn end_ns(&self) -> Option<u64> {
        self.end_ns
    }

    /// Completion status (0 or a caller-defined code).
    #[must_use]
    pub const fn status(&self) -> i32 {
        self.status
    }

    /// True while the span can be closed or parented.
    #[must_use]
    pub const fn is_open(&self) -> bool {
        self.end_ns.is_none()
    }
}

/// A bounded span tracer.
///
/// `enabled == false` is the no-allocation fast path: every start returns `None` and no record is
/// pushed. An enabled tracer records at most `capacity` spans; further sampled spans are dropped
/// and counted. Sampling is decided once per root trace and inherited by children.
#[derive(Clone, Debug)]
pub struct Tracer {
    enabled: bool,
    capacity: usize,
    records: Vec<SpanRecord>,
    dropped: u64,
    next_epoch: u32,
}

impl Tracer {
    /// Create a tracer. A zero capacity behaves as one record.
    #[must_use]
    pub fn new(enabled: bool, capacity: usize) -> Self {
        Self {
            enabled,
            capacity: capacity.max(1),
            records: Vec::new(),
            dropped: 0,
            next_epoch: 1,
        }
    }

    /// Whether this tracer records spans.
    #[must_use]
    pub const fn enabled(&self) -> bool {
        self.enabled
    }

    /// Maximum recorded spans.
    #[must_use]
    pub const fn capacity(&self) -> usize {
        self.capacity
    }

    /// Spans recorded so far (open and closed).
    #[must_use]
    pub fn len(&self) -> usize {
        self.records.len()
    }

    /// Whether no span has been recorded.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.records.is_empty()
    }

    /// Spans dropped because the bounded store was full.
    #[must_use]
    pub const fn dropped(&self) -> u64 {
        self.dropped
    }

    /// The recorded spans in start order.
    #[must_use]
    pub fn records(&self) -> &[SpanRecord] {
        &self.records
    }

    /// Start a root span. Returns `None` when tracing is disabled, the trace is not sampled, or
    /// the bounded store is full.
    pub fn start_root(
        &mut self,
        name: u32,
        now_ns: u64,
        ids: &mut IdGenerator,
        sampler: Sampler,
    ) -> Option<SpanHandle> {
        if !self.enabled {
            return None;
        }
        let trace = ids.next_trace();
        if !sampler.sample(trace) {
            return None;
        }
        let context = SpanContext::root(trace, ids.next_span(), true);
        self.push(context, name, now_ns)
    }

    /// Start a child under `parent`. Returns `Ok(None)` when tracing is disabled or the parent
    /// trace is not sampled; a closed or unknown parent is an error.
    pub fn start_child(
        &mut self,
        parent: SpanHandle,
        name: u32,
        now_ns: u64,
        ids: &mut IdGenerator,
    ) -> Result<Option<SpanHandle>, TraceError> {
        if !self.enabled {
            return Ok(None);
        }
        let parent_context = {
            let record = self.record(parent)?;
            if !record.is_open() {
                return Err(TraceError::ParentClosed);
            }
            record.context
        };
        if !parent_context.sampled {
            return Ok(None);
        }
        let context = parent_context.child(ids.next_span(), true);
        Ok(self.push(context, name, now_ns))
    }

    /// Close one span exactly once.
    pub fn close(
        &mut self,
        handle: SpanHandle,
        now_ns: u64,
        status: i32,
    ) -> Result<(), TraceError> {
        if !self.enabled {
            return Ok(());
        }
        let index = handle.index;
        let record = self.records.get_mut(index).ok_or(TraceError::UnknownSpan)?;
        if record.context.span() != handle.span {
            return Err(TraceError::UnknownSpan);
        }
        if !record.is_open() {
            return Err(TraceError::AlreadyClosed);
        }
        record.end_ns = Some(now_ns);
        record.status = status;
        Ok(())
    }

    fn record(&self, handle: SpanHandle) -> Result<&SpanRecord, TraceError> {
        let record = self
            .records
            .get(handle.index)
            .ok_or(TraceError::UnknownSpan)?;
        if record.context.span() != handle.span {
            return Err(TraceError::UnknownSpan);
        }
        Ok(record)
    }

    fn push(&mut self, context: SpanContext, name: u32, now_ns: u64) -> Option<SpanHandle> {
        if self.records.len() >= self.capacity {
            self.dropped = self.dropped.saturating_add(1);
            return None;
        }
        let epoch = self.next_epoch;
        self.next_epoch = self.next_epoch.wrapping_add(1).max(1);
        let index = self.records.len();
        self.records.push(SpanRecord {
            context,
            name,
            start_ns: now_ns,
            end_ns: None,
            status: 0,
        });
        Some(SpanHandle {
            index,
            span: context.span,
            epoch,
        })
    }
}

/// A handle to one recorded span.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct SpanHandle {
    index: usize,
    span: SpanId,
    epoch: u32,
}

impl SpanHandle {
    /// The slot index in the tracer store (diagnostic only).
    #[must_use]
    pub const fn index(self) -> usize {
        self.index
    }

    /// The span id.
    #[must_use]
    pub const fn span(self) -> SpanId {
        self.span
    }

    /// The record epoch (diagnostic only).
    #[must_use]
    pub const fn epoch(self) -> u32 {
        self.epoch
    }
}

#[cfg(test)]
#[path = "lib_ut.rs"]
mod tests;
