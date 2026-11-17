// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Pure trace id, span and correlation model for cross-process tracing.
//!
//! One request is one [`TraceId`]: generated in the client, stamped on the call by the kernel, and
//! adopted by the server so that every record from client, kernel and server shares the id. This
//! crate is the policy half of that story. It knows nothing about wire formats, syscalls or
//! probe sites; those belong to the later tickets (Q3 stamps `trace_id` on the scheme/IPC header).
//!
//! # Model
//!
//! * [`TraceId`] is a 64-bit id `(boot_nonce:16 | cpu:8 | counter:40)`; zero means "not traced"
//!   (`R-OBS-7`). [`TraceIdGen`] produces them monotonically and never emits zero.
//! * [`Sampling`] is a pure function of the [`TraceId`], decided once per trace and inherited by
//!   every span, so no state and no allocation are needed to keep the decision consistent.
//! * [`TraceContext`] is the value that crosses a process boundary: the caller's `trace_id`,
//!   `span_id` and sampling decision. [`Trace::adopt`] turns it into a local span, correlating the
//!   server's records with the client's.
//! * [`Trace`] is a bounded registry of spans. A span has a parent (local, through
//!   [`Trace::open_child`], or remote, through [`Trace::adopt`]) and a bounded record list. Closing
//!   a span severs its parent link, so a closed span holds no reference to its parent.
//! * [`Tracer`] wraps a [`Trace`] with an enable flag: the disabled path is a branch that returns
//!   `None`/`false` without touching the registry, so disabled tracing does not allocate.
//!
//! # Exactly-once and lifetime
//!
//! A span is open or closed. Only an open span may record or have children opened under it; closing
//! is the terminal transition and cannot be undone. The exhaustive exploration in the tests checks
//! the span lifetime and the "no parent after close" invariant over every reachable state of a
//! small registry.

#![cfg_attr(not(test), no_std)]
#![forbid(unsafe_code)]

extern crate alloc;

mod context;
mod id;
mod sampling;
mod sink;
mod trace;
mod tracer;

pub use context::TraceContext;
pub use id::{COUNTER_BITS, COUNTER_MASK, SpanId, TraceId, TraceIdGen};
pub use sampling::Sampling;
pub use sink::{
    RING_MAGIC, RING_VERSION, RingError, RingHeader, Span, SpanKind, SpanRecord, SpanRing,
    TraceReader, TraceSink,
};
pub use trace::{Error, Limits, Trace};
pub use tracer::Tracer;
