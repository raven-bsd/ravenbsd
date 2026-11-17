# End-to-end trace demo: the scheme → sink → `trace` path

SPDX-License-Identifier: BSD-3-Clause
Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

Status: OBSERVED (Q4b). This proves the scheme → span-sink → `trace`-command path with a dedicated
fixture server before the real `vfsd`/`fsd-ufs`/`blockd` probes land in Q4.

## 1. The path

1. A handler reads the inherited trace id from `raven_scheme::Ctx::trace_id` (the kernel's Q3/Q3b
   stamp lands on the decoded packet and the scheme framework copies it into `Ctx`).
2. The handler records a span (trace id, span id, pid, monotonic timestamp, kind, opcode) into the
   Q4a `SpanRing` in shared memory.
3. A separate `trace` command maps the same sink, drains the committed spans and prints one line
   per span with the trace id, span id, pid, timestamp, kind and status.

The sink is a fixed-capacity multi-producer/single-reader ring (`raven-trace-core::SpanRing`) over a
shared memory object mapped at the fixed address `0x3000_0000` by the controller, the fixture server
and the `trace` command.

## 2. The `Ctx` change

`raven_scheme::Ctx` gains `trace_id: u64`, set from the decoded packet in `Server::dispatch`. The
field is zero when the request is untraced; it costs one `u64` copy and allocates nothing.

## 3. Reproduce it

```
cargo xtask qtest --case trace-demo --arch x86_64
cargo xtask qtest --case trace-demo --arch aarch64
```

The fixture (`tests/fixtures/trace-demo/`) runs `/init`, `/server` and `/trace`:

```
demo: server ready
trace: spans=4 gaps=0 traced=2 untraced=2
trace: id=0xdeadbeefcafef00d span=0x1 parent=0x0 pid=2 ts=236864471 kind=server op=1
trace: id=0xdeadbeefcafef00d span=0x2 parent=0x0 pid=2 ts=253034408 kind=server op=2
trace: id=untraced span=0x3 parent=0x0 pid=2 ts=262914730 kind=server op=1
trace: id=untraced span=0x4 parent=0x0 pid=2 ts=265078015 kind=server op=2
demo: ok
```

The controller sets `0xdeadbeefcafef00d` with `TRACE_SET`, opens and reads the fixture scheme (the
server records a span per data-path request with the inherited id), clears the id and repeats the
call untraced. The `op` field is the wire opcode (1 = open, 2 = read).

## 4. Remaining real-server probes (Q4, after P5e/P5g)

The real `vfsd`, `fsd-ufs` and `blockd` probes are Q4's step once their paths are free. Each server
adds the same one-line record at its open/read/reply entry, into the same sink, and the `trace`
command prints the client → `vfsd` → `fsd-ufs` → `blockd` tree. The kernel's last-close
notification (generated in the server's own RECV context) carries no caller trace id, so the demo
records the data-path requests (open/read), not the close.
