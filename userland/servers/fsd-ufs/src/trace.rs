// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Minimal fsd-ufs span probe.
//!
//! The probe is a safe producer over the runtime sink: a zero trace id or a missing sink is a
//! no-op and allocates nothing. The launcher transfers the sink as handle 3 and the runtime maps
//! it once; this module only builds spans from the request's inherited id.

use raven_abi::types::clock;
use raven_rt::trace::{SINK_HANDLE, Sink};
use trace::{Span, SpanKind};

/// This server's sink producer slot (the fixture server owns 0).
const PRODUCER: u32 = 2;
/// Userspace has no getpid; the kernel owns pids, so real-server spans carry 0.
const PID: u32 = 0;

/// A sink handle plus the per-server span sequence.
pub struct Probe {
    sink: Sink,
    sequence: u64,
}

impl Probe {
    /// Attach the server probe.
    pub fn server() -> Self {
        Self {
            sink: Sink::attach(SINK_HANDLE, PRODUCER),
            sequence: 0,
        }
    }

    /// Record one server span for `opcode`; a zero id or disabled sink does nothing.
    pub fn record(&mut self, trace_id: u64, opcode: u16) {
        if trace_id == 0 || !self.sink.enabled() {
            return;
        }
        let timestamp_ns = raven_rt::sys::clock_get(clock::MONOTONIC)
            .map(|ts| {
                (ts.tv_sec.max(0) as u64)
                    .saturating_mul(1_000_000_000)
                    .saturating_add(ts.tv_nsec.max(0) as u64)
            })
            .unwrap_or(0);
        let span = Span {
            trace_id,
            span_id: self.sequence.saturating_add(1),
            parent_span_id: 0,
            timestamp_ns,
            pid: PID,
            kind: SpanKind::Server,
            opcode,
        };
        let _ = self.sink.emit(span);
        self.sequence = self.sequence.saturating_add(1);
    }
}
