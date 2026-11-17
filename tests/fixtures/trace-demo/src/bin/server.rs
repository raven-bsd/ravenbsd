// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Q4b trace-demo server: registers `trace-demo:` and records one span per request into the
//! shared sink, using the trace id inherited from the caller (`Ctx.trace_id`).

#![no_std]
#![no_main]

use raven_abi::types::{Handle, clock};
use raven_rt::entry;
use raven_rt::println;
use raven_scheme::{Ctx, Handler, HandlerResult, Opened, Server};
use raven_trace_core::{Span, SpanKind, SpanRing};
use raven_trace_demo::{map_sink, sink_refs};

entry!(main);

/// The sink memory object transferred to this process before its first thread.
const SINK_HANDLE: Handle = Handle::new(3, 0);
/// Logical pid reported for the fixture server.
const SERVER_PID: u32 = 2;

/// The server handler: one span per open/read/close, then a byte for read.
struct TraceHandler {
    ring: SpanRing<'static>,
    sequence: u64,
}

impl TraceHandler {
    fn record(&mut self, ctx: &Ctx, opcode: u16) {
        let timestamp_ns = raven_rt::sys::clock_get(clock::MONOTONIC)
            .map(|ts| {
                (ts.tv_sec.max(0) as u64)
                    .saturating_mul(1_000_000_000)
                    .saturating_add(ts.tv_nsec.max(0) as u64)
            })
            .unwrap_or(0);
        let span = Span {
            trace_id: ctx.trace_id,
            span_id: self.sequence.saturating_add(1),
            parent_span_id: 0,
            timestamp_ns,
            pid: SERVER_PID,
            kind: SpanKind::Server,
            opcode,
        };
        let _ = self.ring.emit(0, self.sequence, &span);
        self.sequence = self.sequence.saturating_add(1);
    }
}

impl Handler for TraceHandler {
    fn open(&mut self, _path: &str, _flags: u32, _mode: u32, ctx: &Ctx) -> HandlerResult<Opened> {
        self.record(ctx, 1);
        Ok(Opened::File(1))
    }

    fn read_with(
        &mut self,
        _file: u64,
        _offset: u64,
        buffer: &mut [u8],
        ctx: &Ctx,
    ) -> HandlerResult<usize> {
        self.record(ctx, 2);
        match buffer.first_mut() {
            Some(slot) => {
                *slot = b'R';
                Ok(1)
            }
            None => Ok(0),
        }
    }

    fn close_with(&mut self, _file: u64, _ctx: &Ctx) -> HandlerResult<()> {
        // The kernel's last-close notification is generated in the server's own RECV context and
        // carries no caller trace id; this fixture records the data-path requests (open/read).
        Ok(())
    }
}

fn main(_args: raven_rt::Args) -> i32 {
    if map_sink(Handle::SELF_PROC, SINK_HANDLE).is_err() {
        println!("demo: sink map failed");
        return 1;
    }
    // SAFETY: the sink was mapped above at the shared fixed address.
    let (header, records) = unsafe { sink_refs() };
    let ring = match SpanRing::new(header, records) {
        Ok(ring) => ring,
        Err(_) => {
            println!("demo: sink header failed");
            return 1;
        }
    };
    let mut server = match Server::register("trace-demo") {
        Ok(server) => server,
        Err(_) => {
            println!("demo: register failed");
            return 1;
        }
    };
    println!("demo: server ready");
    server.run(&mut TraceHandler { ring, sequence: 0 });
}
