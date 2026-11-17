# End-to-end trace demo: the scheme → sink → `trace` path

SPDX-License-Identifier: BSD-3-Clause
Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

Status: OBSERVED (Q4e, reconfirmed in the Q4 close-out on 2026-11-17). The Q4b fixture server still
proves the scheme → span-sink → `trace` path, and the same demo stages the real `initfsd`,
`blockd`, `fsd-ufs` and `vfsd` with the sink transferred. A mounted `file:/mnt/ten.txt` read
records client → `vfsd` → `fsd-ufs` → `blockd` spans under one trace id. The direct `ufs:/ten.txt`
read stays in the demo.

Q4 close-out: `cargo xtask qtest --case trace-demo` passes on x86_64 and aarch64. Both runs record
13 spans with `gaps=0` (11 traced, 2 untraced) in the same order, so there is no orphan span. The
x86_64 transcript below is from the earlier Q4e observation; the close-out run differs only in the
monotonic timestamps.

## 1. The path

1. A handler reads the inherited trace id from `raven_scheme::Ctx::trace_id` (the kernel's Q3/Q3b
   stamp lands on the decoded packet and the scheme framework copies it into `Ctx`) or from the
   polled packet.
2. The handler records a span (trace id, span id, pid, monotonic timestamp, kind, opcode) into the
   Q4a `SpanRing` in shared memory.
3. A separate `trace` command maps the same sink, drains the committed spans and prints one line
   per span with the trace id, span id, pid, timestamp, kind and status.

The sink is a fixed-capacity multi-producer/single-reader ring (`raven-trace-core::SpanRing`) over a
shared memory object mapped at the fixed address `0x3000_0000` by the controller, the fixture
server, the real servers and the `trace` command. The producer slots are: fixture server 0,
`vfsd` 1, `fsd-ufs` 2, `blockd` 3.

## 2. The `Ctx` change

`raven_scheme::Ctx` gains `trace_id: u64`, set from the decoded packet in `Server::dispatch`. The
field is zero when the request is untraced; it costs one `u64` copy and allocates nothing.

## 3. Reproduce it

```
cargo xtask qtest --case trace-demo --arch x86_64
cargo xtask qtest --case trace-demo --arch aarch64
```

The fixture (`tests/fixtures/trace-demo/`) packs `/init`, `/server`, `/trace`, `/initfsd`,
`/blockd`, `/fsd-ufs`, `/vfsd`, `/etc/mounts` and `/etc/blockd.img`. The controller starts the real
stack (initfsd → blockd → fsd-ufs → vfsd, sink transferred as handle 3), then performs a traced
mounted `file:/mnt/ten.txt` read while the UFS blocks are cold, the same read untraced, a traced
direct `ufs:/ten.txt` read and an untraced one, and runs `trace`. Observed transcript, x86_64
(`build/qtest/trace-demo.log`):

```
vfsd: mounted / on initfs:
vfsd: mounted /mnt on ufs:
trace: spans=13 gaps=0 traced=11 untraced=2
trace: id=0xdeadbeefcafef00d span=0x1 parent=0x0 pid=2 ts=32627872559 kind=server op=1
trace: id=0xdeadbeefcafef00d span=0x2 parent=0x0 pid=2 ts=32632476430 kind=server op=2
trace: id=untraced span=0x3 parent=0x0 pid=2 ts=32640632770 kind=server op=1
trace: id=untraced span=0x4 parent=0x0 pid=2 ts=32643313597 kind=server op=2
trace: id=0xdeadbeefcafef00d span=0x1 parent=0x0 pid=0 ts=32650096555 kind=server op=1
trace: id=0xdeadbeefcafef00d span=0x1 parent=0x0 pid=0 ts=32652460788 kind=server op=1
trace: id=0xdeadbeefcafef00d span=0x2 parent=0x0 pid=0 ts=32662473269 kind=server op=1
trace: id=0xdeadbeefcafef00d span=0x3 parent=0x0 pid=0 ts=32667657090 kind=server op=1
trace: id=0xdeadbeefcafef00d span=0x2 parent=0x0 pid=0 ts=32671002620 kind=server op=2
trace: id=0xdeadbeefcafef00d span=0x4 parent=0x0 pid=0 ts=32672170532 kind=server op=2
trace: id=0xdeadbeefcafef00d span=0x1 parent=0x0 pid=0 ts=32674412523 kind=io op=7
trace: id=0xdeadbeefcafef00d span=0x5 parent=0x0 pid=0 ts=32704321255 kind=server op=1
trace: id=0xdeadbeefcafef00d span=0x6 parent=0x0 pid=0 ts=32707643924 kind=server op=2
demo: ok
```

The aarch64 run records the same 13 spans in the same order with different timestamps. The
controller sets `0xdeadbeefcafef00d` with `TRACE_SET`; the fixture server records `pid=2` spans
(open/read), and the real servers record `pid=0` because userspace has no `getpid` (the kernel
owns pids).

Span mapping for the mounted read: the first `pid=0` `server op=1` is `vfsd` open (producer 1
span 1); the next three `server op=1` lines are the `fsd-ufs` opens behind it (producer 2 spans
1-3: vfsd's access checks and target open); `vfsd` read is producer 1 span 2; `fsd-ufs` read is
producer 2 span 4; and `blockd` records the nested block I/O as producer 3 span 1 (`io op=7`).
The last two `server op=1/op=2` lines are the direct `ufs:` read on the now-warm cache (`fsd-ufs`
spans 5 and 6); it records no `blockd` span because no block was re-read.

## 4. Notes

* `vfsd` records open and read spans from `raven_scheme::Ctx.trace_id` through the safe
  `raven_rt::trace` API. A zero trace id or a missing sink is a no-op, so the two untraced reads
  add no server spans; only the fixture server records its two untraced proof spans.
* The `vfsd` child is forked with a snapshot of the registered scheme names. The controller waits
  for `ufs:/ten.txt` before staging `vfsd`, otherwise `ufs:` would be missing from that snapshot
  and vfsd would skip the `/mnt` mount.
* The kernel's last-close notification (generated in the server's own RECV context) carries no
  caller trace id, so the demo records the data-path requests (open/read), not the close.
