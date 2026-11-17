# End-to-end trace demo: the scheme → sink → `trace` path

SPDX-License-Identifier: BSD-3-Clause
Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

Status: OBSERVED (Q4c). The Q4b fixture server still proves the scheme → span-sink → `trace`
path, and the same demo now stages the real `initfsd`, `blockd` and `fsd-ufs` with the sink
transferred. A real `ufs:/ten.txt` read records client → `fsd-ufs` → `blockd` spans under one
trace id. The mounted `file:` path still needs `vfsd` (P5c/Q4e); this demo opens `ufs:` directly.

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

The fixture (`tests/fixtures/trace-demo/`) packs `/init`, `/server`, `/trace`, `/initfsd`,
`/blockd`, `/fsd-ufs` and `/etc/blockd.img`. The controller runs the fixture proof, then starts the
real block stack (initfsd → blockd → fsd-ufs, sink transferred as handle 3) and reads
`ufs:/ten.txt`. Observed transcript, aarch64 (`build/qtest/trace-demo.log`):

```
demo: server ready
blockd: gpt ok partitions=1
blockd: part 1 ufs-root 34..32801
fsd-ufs: mounted block:0 partition 1 as ufs: (16777216 bytes)
fsd-ufs: mode=rw journal=none
trace: spans=10 gaps=0 traced=8 untraced=2
trace: id=0xdeadbeefcafef00d span=0x1 parent=0x0 pid=2 ts=17180062048 kind=server op=1
trace: id=0xdeadbeefcafef00d span=0x2 parent=0x0 pid=2 ts=17185575456 kind=server op=2
trace: id=untraced span=0x3 parent=0x0 pid=2 ts=17192703552 kind=server op=1
trace: id=untraced span=0x4 parent=0x0 pid=2 ts=17197691968 kind=server op=2
trace: id=0xdeadbeefcafef00d span=0x1 parent=0x0 pid=0 ts=18705188032 kind=server op=1
trace: id=0xdeadbeefcafef00d span=0x1 parent=0x0 pid=0 ts=18708408880 kind=io op=7
trace: id=0xdeadbeefcafef00d span=0x2 parent=0x0 pid=0 ts=18714389968 kind=io op=7
trace: id=0xdeadbeefcafef00d span=0x3 parent=0x0 pid=0 ts=18719286480 kind=io op=7
trace: id=0xdeadbeefcafef00d span=0x2 parent=0x0 pid=0 ts=18725814448 kind=server op=2
trace: id=0xdeadbeefcafef00d span=0x4 parent=0x0 pid=0 ts=18727523712 kind=io op=7
demo: ok
```

The controller sets `0xdeadbeefcafef00d` with `TRACE_SET`, opens and reads the fixture scheme, then
`ufs:/ten.txt`, clears the id, and repeats both calls untraced. The fixture server records
`pid=2` spans (open/read); the real servers record `pid=0` because userspace has no `getpid` (the
kernel owns pids). `fsd-ufs` records server open/read, and `blockd` records the nested block I/O
with the same trace id (`op` is the wire opcode: fsd-ufs 1 = open, 2 = read; blockd 7 = call).
The x86_64 run records the same tree with different timestamps.

## 4. Remaining: the vfsd span (P5c/Q4e)

The real `fsd-ufs` and `blockd` probes are landed above. The mounted `file:` path still goes
through `vfsd`, which has no probe; adding it would give the client → `vfsd` → `fsd-ufs` →
`blockd` tree for a normal mounted read. `vfsd` is owned by P5c and is Q4e. This demo reaches the
two servers through the direct `ufs:` scheme instead. The kernel's last-close notification
(generated in the server's own RECV context) carries no caller trace id, so the demo records the
data-path requests (open/read), not the close.
