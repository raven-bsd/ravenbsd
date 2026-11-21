<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# posix-fs-smp worker stall investigation (K391)

Status: K391, 2026-11-21. **Gate is BLOCKED**: the worker stall is intermittent and not yet
pinned to a component inside this ticket's Owns. Evidence-only changes are in the fixture; no
kernel or S159 fix path was edited.

## 1. Reproduction

S159 round-3 matrix: 3 of 14 TCG4 runs failed with `smp-fs: failed worker timeout pending=[...]`,
one at host load 1.33 after 204 s, and **no TLB line** in any failure. The K391 run reproduced a
stall at low load:

```text
smp-fs: failed worker stall slot=0 state=0x20d
pending=["0:13:2", "1:14:2", "2:14:2", "3:14:2", "4:31:3", ...]
```

The new progress state is `phase:iteration`; phase 13 = `written`, 14 = `read`, 24 = `mmap sync`,
31 = mount worker after mount before write. In one run the stall was `state=0xa18` for every file
worker (iteration 10, phase 24 = mmap sync); in another it was iteration 2, phases 13/14, with
mount workers at 31. The mount workers finish in the S159 failures; in the K391 early stall they
also stop, so the stall can fan out through the shared race barrier.

## 2. Trace

The fixture now prints:

* `setup mkdir ...` / `setup mkdir ok ...` markers;
* `progress finished=N/8 pending=["worker:phase:iteration", ...]` every 5 s;
* a 120 s per-worker no-progress check that fails with `worker stall slot=N state=...`;
* a 240 s total deadline (measured: 151 s green run, 44/50 iterations at 180 s) instead of the
  old 180 s deadline.

With `VFSD_TRACE=1`, the passing runs show vfsd answering `open`, `read`, `write`, `mmap` and
`mmap-reply` normally; the trace run that passed has no lost reply. No trace run reproduced the
stall, so the last vfsd request during a stall is not captured yet. The failed run has no
`smp: tlb shootdown timeout` line, and the S113 reproducer is green 10/10 (`tlb_stale=0`), so the
TLB delivery is not implicated.

Current evidence narrows the stall to a complete stop of all file workers (and sometimes mount
workers) in vfsd-backed operations, with no kernel TLB line and no fixture progress. It is not
simple slowness: `mmap_us` p90 is 3.37 s and the no-progress bound is 120 s.

## 3. Decision

The stalled operation is in the `file:` path. The fixture is not the cause (it shows a hard
no-progress stop; the old 180 s deadline is also extended to 240 s with a separate 120 s forward
progress check). The remaining candidates are:

1. a vfsd/ramfsd handler/wait-path defect, or
2. a kernel wait/wakeup/IPC delivery defect behind the `file:` call.

K391's Owns cover the fixture, vfsd/ramfsd, the case files and this document; the kernel wait/wakeup
path is outside its Owns. The audit therefore stops at the component boundary and hands the trace
to the kernel/owner rather than guessing a kernel patch.

## 4. Changes made

* `tests/posix/fs/posix-fs/src/bin/smp.rs`: setup/progress markers with iteration, 240 s measured
  total deadline, 120 s per-worker no-progress stall detector, optional `VFSD_TRACE`/`RAMFSD_TRACE`
  env.
* No test weakening: the stall detector fails a hard stop faster than the old timeout and reports
  the exact phase/iteration; a slow but progressing run is allowed.
* No qtest case timeout, kernel file or S159 fix path was changed.

## 5. Evidence

| Check | Result |
|---|---|
| S159 r3 matrix | 3/14 fail, no TLB line, one at load 1.33 |
| K391 stall | `slot=0 state=0x20d` / all file workers at phase 24 in another run |
| vfsd trace passing run | normal open/read/write/mmap replies, no lost reply |
| `smp-boot` | PASS |
| `lock-time repro` | 10/10 PASS |
| S113 TLB repro | 10/10 PASS, `tlb_stale=0`, worst 4.98 s |
| 20/20 TCG4 green | **not achieved**; the stall is intermittent |

## 6. Gaps

* A stall was not captured with `VFSD_TRACE`; the last vfsd request during the stop is still
  missing. Need a trace-enabled run that stalls, or a kernel-side wait-queue dump for the stuck
  `file:` call.
* Component decision is between vfsd/ramfsd and kernel wait/wakeup; K391 Owns do not include the
  kernel wait path, so a kernel fix needs the owning lane.
* The full 20/20 TCG4 acceptance remains blocked until the stall is fixed.
