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
| 20/20 TCG4 green | **achieved in rework 2** (build + 19 consecutive `--no-build` runs, no stall) |

## 6. Stall diagnostic run (rework 1)

The fixture now dumps `file:__diag__` on a stall; `vfsd` records a bounded 16-entry op ring and
prints it on that path. A hard stall was captured at low load (start 8.8, end 11.0):

```text
smp-fs: progress finished=0/8 pending=["0:23:26", "1:24:26", "2:24:26", "3:24:26", "4:37:36", ...]
smp-fs: diag request start
```

The diagnostic request itself never completes, and the vfsd serial has no further `vfsd:` or
`vfsd-diag:` line after startup. All file workers are stopped in mmap phases (23/24) and the mount
workers in mount phases (37/38). Because vfsd is a single-threaded server, the queued diagnostic
open cannot be served while its active handler is blocked; this classifies the visible state as
**vfsd/ramfsd reply missing or vfsd blocked in a forwarded `file:` call**, not a fixture deadline
and not a TLB event. The exact last forwarded request (mount/umount vs mmap/read) and the kernel
wait queue of the blocked thread are still not captured; the vfsd ring is written but cannot be
printed from the stuck server.

## 7. Root cause and fix (rework 2)

`PROC_PID_INFO` was extended with process-scoped diagnostic words (state, runnable, last syscall,
pc, live wait object, in-syscall count, last submit/recv/send request ids, client/queue/active
depths, `scheme_waiters` length and key). A hard stall then showed:

```text
smp-fs: proc-info vfsd  ... syscall=0x61 wait=0x930 in_syscall=1
smp-fs: proc-req  vfsd  ... submit=1328 client=1
smp-fs: proc-info ramfsd ... syscall=0x68 wait=0x2c8 in_syscall=1
smp-fs: proc-req  ramfsd ... recv=1327 send=1327 queue=1 waiters=0
smp-fs: proc-wake ramfsd ... fail=0 pending=1 last_cpu=resumed
```

vfsd is blocked in `OPEN` waiting for request 1328; ramfsd has 1328 queued, is blocked in `RECV`,
and has **no registered waiter**. The wake did not fail: `pending=1` means the waker saw the
server thread as `Running`, stored `pending_wake`, and the server then blocked without consuming
it. The state read and the `pending_wake` store are separate operations, so on another CPU they
can be reordered against the waiter's final `pending_wake` check and `schedule_locked`. This lost
wakeup is permanent and matches the fixture's 120 s no-progress stall.

Fix: `Thread::block_lock` serialises the handshake. `sched::wake` holds it while it reads the
thread state and publishes `pending_wake`; `block_current`/`sleep_until` hold it while they set
the state and perform the final `pending_wake` check, and release it before `schedule_locked`.
The lock is held only for the read-modify sequence, never across a context switch.

## 8. Gaps

* The stall diagnostics are process-scoped. The fixture's workers are threads of one process, so
  their individual kernel syscall/pc is not isolated; the fixture's own `phase:iteration` markers
  remain the per-worker evidence.
* `make check-fast` skips the kernel package as not host-testable; kernel changes are exercised by
  qtest (`posix-fs-smp`, `smp-boot`) and the S113/lock-time harnesses.
* Three post-fix runs failed only on the fixture's 240 s deadline or on a forbidden TLB-shootdown
  line at host load ~20; none showed a worker stall and the final 20/20 batch was clean.
