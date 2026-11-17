# Cross-process tracing: id, span and correlation

SPDX-License-Identifier: BSD-3-Clause
Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

Status: OBSERVED (implements the pure core of `architecture/kernel-next/06-observability.md`
R-OBS-6..7 and R-OBS-11). This file describes `crates/raven-trace-core`; stamping `trace_id` on a
wire header and the kernel propagation are Q3.

## 1. What this is

Cross-process tracing is the differentiator: one request, one id, from a client syscall through the
kernel router into `vfsd`, `fsd-ufs` and `blockd`, and back. `raven-trace-core` is the pure policy
half of that story: the id, the span tree, the sampling decision, the client/server correlation and
the bounds. It has no protocol field, no syscall number and no kernel dependency; those are Q3.

## 2. The id (`R-OBS-7`)

`TraceId` is 64 bits laid out `boot_nonce:16 | cpu:8 | counter:40`. Zero means "not traced".
`TraceIdGen` produces them monotonically per boot/CPU and never emits zero (the counter wraps to 1,
not 0). `SpanId` is a per-registry span identifier; zero is "no span".

## 3. Sampling

`Sampling` is `Never`, `Always`, or `Every(n)`. `decide` is a pure function of the `TraceId`, so
every span of a trace — which shares the id — observes the same decision with no shared state and
no allocation. The decision is decided once at the trace root and inherited by children.

## 4. Spans and lifetime

A `Trace` is a bounded registry of spans. A span is *open* or *closed*:

| State | Allowed | Forbidden |
|---|---|---|
| open | `record`, `open_child` under it | — |
| closed | (terminal) | `record`, `open_child` under it, second `close` |

Closing a span severs its parent links: a closed span reports no local `parent` and no
`remote_parent`, so it holds no reference to its parent and the parent can be reclaimed. A child can
only be opened under an *open* parent. Spans are never removed, so a closed span's records stay
readable until the registry is dropped.

## 5. Correlation of client/server transitions (`R-OBS-6`)

`TraceContext { trace_id, span_id, sampled }` is the value that crosses a process boundary:

* A client starts a new trace: `generator.next_id()` → `TraceContext::new(id, SpanId::NONE, sampler.decide(id))`.
* The kernel stamps `trace_id` on the call; the server thread's current `trace_id` becomes the
  caller's for the duration of the call.
* The server calls `Trace::adopt(caller_context)`: the new span gets the caller's `trace_id` and
  sampling decision, and the caller's `span_id` becomes its *remote parent*. Records from the
  client, the kernel and the server therefore share one `trace_id` — the join key — with no
  per-subsystem plumbing.

## 6. Bounds

`Limits { max_spans, max_records }` (defaults 1024 and 64; clamped to `1..=4096`). A span's record
list is bounded: when full, the oldest record is dropped and the span's drop count advances (the
dtrace ring policy). The registry is bounded: `adopt`/`open_child` fail with `TooManySpans` when
full.

## 7. No-alloc fast path when disabled (`R-OBS-11`)

`Tracer` wraps a `Trace` with an enable flag. When tracing is disabled — the flag is off, or the
incoming `TraceContext` has a zero `trace_id` — every operation returns `None`/`false` *before*
touching the registry, so the disabled path allocates nothing. This is the pure form of the
zero-cost-when-disabled branch; the kernel enable word and probe-site patching are later tickets.

## 8. Exhaustive model

The test `exhaustive_span_lifetime_preserves_no_parent_after_close` explores the reachable
transition system of a registry with 3 spans and 2 records per span, applying every enabled
operation (adopt of four contexts, open_child/close/record over four candidate ids) from every
state, deduplicating states and bounded to 8 transitions per path. At every state it checks:
spans and records are bounded; no span has a zero trace id; and no closed span retains a parent or
remote parent. The state/transition counts are printed by the test.
