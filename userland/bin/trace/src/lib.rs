// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Read the cross-process span sink and print one request tree.
//!
//! The sink is a [`raven_trace_core::SpanRing`] in shared memory; this crate holds the layout
//! constants shared with the fixture, the drain/format logic (host-tested here) and the target
//! binary that maps the sink and prints.

#![cfg_attr(not(test), no_std)]

extern crate alloc;

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;

use raven_trace_core::{RingError, SpanRing};

pub use raven_trace_core::{Span, SpanKind};

/// Sink ring capacity (records).
pub const CAPACITY: u32 = 64;
/// Number of producers writing the sink.
pub const PRODUCERS: u32 = 4;
/// Fixed address where every process maps the sink.
pub const SINK_ADDR: usize = 0x3000_0000;
/// Size of the ring header at the base of the sink.
pub const HEADER_SIZE: usize = core::mem::size_of::<raven_trace_core::RingHeader>();
/// Size of one record.
pub const RECORD_SIZE: usize = core::mem::size_of::<raven_trace_core::SpanRecord>();
/// Total sink bytes (header + records).
pub const SINK_BYTES: usize = HEADER_SIZE + CAPACITY as usize * RECORD_SIZE;
/// Sink bytes rounded up to whole pages.
pub const SINK_PAGES: usize = SINK_BYTES.div_ceil(4096);

/// Drain every committed span from `ring`, returning the spans and the gap count.
pub fn read_spans(ring: &SpanRing<'_>) -> (Vec<Span>, u64) {
    let mut spans = Vec::new();
    let mut cursor = 0u64;
    let mut gaps = 0u64;
    loop {
        match ring.read(&mut cursor) {
            Ok(span) => spans.push(span),
            Err(RingError::Gap { missed }) => gaps = gaps.saturating_add(missed),
            Err(_) => break,
        }
    }
    (spans, gaps)
}

/// Format the span tree: a summary line (span count, gaps, traced/untraced) plus one line per
/// span with the trace id, span id, parent, pid, timestamp, kind and opcode. A zero trace id
/// prints as `untraced`.
pub fn format_tree(spans: &[Span], gaps: u64) -> String {
    let traced = spans.iter().filter(|span| span.trace_id != 0).count();
    let untraced = spans.len().saturating_sub(traced);
    let mut out = String::new();
    out.push_str(&format!(
        "trace: spans={} gaps={} traced={} untraced={}\n",
        spans.len(),
        gaps,
        traced,
        untraced
    ));
    for span in spans {
        let id = if span.trace_id == 0 {
            String::from("untraced")
        } else {
            format!("0x{:x}", span.trace_id)
        };
        out.push_str(&format!(
            "trace: id={} span=0x{:x} parent=0x{:x} pid={} ts={} kind={} op={}\n",
            id,
            span.span_id,
            span.parent_span_id,
            span.pid,
            span.timestamp_ns,
            kind_name(span.kind),
            span.opcode,
        ));
    }
    out
}

/// Stable text name for a span kind.
pub const fn kind_name(kind: SpanKind) -> &'static str {
    match kind {
        SpanKind::Client => "client",
        SpanKind::Server => "server",
        SpanKind::Kernel => "kernel",
        SpanKind::Io => "io",
    }
}

#[cfg(test)]
#[path = "lib_ut.rs"]
mod tests;
