<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# KVM `-smp 2` pre-stdio timeout: classification

Date: 2026-11-18. Ticket: S123 (Otto). Depends: S112, S119. Cross-references: S107/S116
(exclusive-host gate), S113 (kernel thread/TLB), S104 (libc threads).

## Summary

The KVM2 `smp-libc-concurrency-kvm2` case timed out twice for S119 at 14:41-14:43Z after
`smp-libc-concurrency: start`, with `smp: 2 cpus online` and all services up, and then passed on
a clean rerun (28.6 s). S123 ran the case **10 times** on `origin/master` `36e8fd8c` (S119's stdio
lock fix merged): **10/10 pass, 14.1-41.5 s**, under load1 14.2-31.8 and 3-8 concurrent QEMU
processes. The failure did **not** reproduce.

It is not classified as a plain host-contention pass: the preserved run happened in a
demonstrably oversubscribed window (S107 measured load1 22.8-84.6 with other QEMU/xtask/rustc at
14:11-14:29Z, ~5 minutes before the timeout), and the merged stdio lock waits with an unbounded
spin and a no-op wake, which is contention-sensitive. But 10/10 passes under moderate load mean
host load alone is not sufficient evidence. Final verdict is deferred to the exclusive window in
S116/S107; if it recurs there, the log goes to S119/Phil (spin wait) and S113/Stuart (kernel
thread creation).

## Preserved failure (exact log)

Source: `/tmp/opencode/s119-r2-kvm2b.log` (S119 rework, saved by Phil). Build string:
`RavenBSD kernel 0.1 (x86_64) eb3ac161-dirty 2026-11-18` — the pre-merge tree with S119's stdio
patch applied, before commit `1153c02d`.

```text
acpi: 2 cpus, 1 ioapics, 5 overrides
timer: 1000 Hz
smp: 2 cpus online
process: starting /init
sched: running (2 cpus, slice 10 ms)
svcd: vfsd up
smp-libc-concurrency: start
--- result ---
timed out after 120 s (expected success); unmatched expectations:
^smp-libc-concurrency: malloc ok$, ^smp-libc-concurrency: stdio ok lines=16000$,
^smp-libc-concurrency: done$, ^svcd: settled$, ^init exited \(0\)$
```

What the serial proves: the guest booted fully (`acpi: 2 cpus`, `smp: 2 cpus online`,
`sched: running (2 cpus, slice 10 ms)`, all services up, `/init` running) and reached the fixture's
first line. It then produced no further output for the 120 s case timeout.

What it does not prove: the fixture prints nothing per worker on success, and only prints
`malloc ok` after all 8 joins. A stall in `pthread_create`, in `pthread_join`, or inside the
worker loop (malloc/stdio) all look identical from the serial. "Before any stdio operation" is
therefore an inference, not an observation.

## 10-run campaign

Command, run 10 times in the foreground, exactly as the ticket states:

```
cargo xtask qtest --case smp-libc-concurrency-kvm2 --arch x86_64
```

Each run's serial and JSON are preserved as `/tmp/opencode/s123/run-<N>.log` and
`run-<N>.json`; the summary is `/tmp/opencode/s123/runs.tsv`.

| Run | UTC | load1 before | other QEMU | Result | Duration | Guest CPU state |
|---:|---|---:|---:|---|---:|---|
| 1 | 14:51:33 | 16.80 | 5 | pass | 41.5 s | `acpi: 2 cpus`, `smp: 2 cpus online` |
| 2 | 14:52:16 | 31.84 | 4 | pass | 20.9 s | same |
| 3 | 14:52:38 | 27.47 | 3 | pass | 18.0 s | same |
| 4 | 14:52:57 | 26.41 | 6 | pass | 19.3 s | same |
| 5 | 14:53:17 | 22.41 | 8 | pass | 35.7 s | same |
| 6 | 14:53:53 | 15.82 | 7 | pass | 14.1 s | same |
| 7 | 14:54:08 | 14.25 | 4 | pass | 17.3 s | same |
| 8 | 14:54:26 | 14.73 | 6 | pass | 21.6 s | same |
| 9 | 14:54:48 | 14.77 | 8 | pass | 23.2 s | same |
| 10 | 14:55:13 | 17.78 | 5 | pass | 16.3 s | same |

Durations (JSON `duration_ms`): n=10, min 14079 ms, **p50 19256 ms, p90 35659 ms, max 41451 ms**.
Every run contains all of:

```text
acpi: 2 cpus, 1 ioapics, 5 overrides
smp: 2 cpus online
sched: running (2 cpus, slice 10 ms)
smp-libc-concurrency: start
smp-libc-concurrency: malloc ok
smp-libc-concurrency: stdio ok lines=16000
smp-libc-concurrency: done
init exited (0)
```

No run logged `cpu 1 did not start`, `panic`, `EXCEPTION`, `timed out` or any forbidden line.

## Host state and exclusive preflight

The host is shared with other minions. The S116 exclusive preflight
(`scripts/boot-direct.sh --exclusive --preflight-only --mode full`) refused **before** and
**after** the campaign; no quiet window existed, so the quiet-host branch of the ticket could not
be exercised:

```text
14:50:57Z exit 2: busy: QEMU process 4122457 (another minion's disk-tools case)
           nproc=20 load1=6.38  qemu_processes=3 sample_count=0
14:56:04Z exit 2: nproc=20 load1=11.80 qemu_processes=5 sample_count=0
```

JSON: `/tmp/opencode/s123-preflight/preflight-20261118T145057Z-4128104.json`,
`/tmp/opencode/s123-preflight2/preflight-20261118T145604Z-17403.json`.

For the preserved timeout window, S107's report records that the shared host never went idle:

```text
14:11:32Z soak: TIMEOUT after 900 s; load 84.56 -> 22.80; other qemu/xtask/cargo/rustc;
          preflight refused, zero boots run
14:26:32Z kernel gate: REFUSE load=27.86>2.0, qemu/xtask present; exit 2
```

## Analysis

1. The guest was healthy: both APs online, scheduler running on 2 CPUs, all schemes mounted,
   `/init` executing. This rules out the S83 AP bring-up and the S107 OVMF-only boot failure.
2. The stall is after the fixture's `start`, before the first success marker. The fixture creates
   8 workers and joins them; any of thread creation, join, or the worker loop can be the stall.
3. S119's merged lock (`libc/ravenlibc/crates/raven-libc/src/ffi/env.rs:131-137`) waits by
   **spinning with no bound, yield or park**, and `wake` is a no-op. Eight workers on a 2-vCPU
   guest, on a host that S107 measured at load 22-85 with 3-9 other QEMU processes, can keep a
   lock owner off-CPU long enough to stretch a run past the 120 s case timeout. This is a
   *plausible mechanism*, not a proven cause: the 10 current runs on load 14-32 all passed.
4. The case timeout is 120 s, which is only ~3x the p90 (35.7 s) and ~3x the preserved passing
   rerun (28.6 s). The timeout bound was **not** changed and must not be changed to hide this.

## Verdict and hand-off

- **Classification: host-contention-sensitive intermittent stall, not reproduced; final verdict
  deferred to the S116/S107 exclusive window.** Not enough evidence to call it a plain kernel bug,
  and not enough to call it pure host noise.
- The exact preserved serial and the 10-run evidence are in this document and in
  `/tmp/opencode/s123/`. If it recurs on a quiet/exclusive host, hand the serial to
  **S119/Phil** (unbounded spin wait in `ShellEnv::wait`, no-op `wake`) and **S113/Stuart**
  (kernel thread creation/wakeup) as a real pre-stdio flake. If it only recurs at high load,
  S116/S107 should record it as host-contention evidence.
- No fixture, threshold, timeout or expectation was changed. No failed run was retried into a
  pass: the campaign had no failure, and the preserved two failures remain failures in S119's
  record.

## Reproduction

```
cargo xtask libc-pack --arch x86_64
cargo xtask qtest --case smp-libc-concurrency-kvm2 --arch x86_64
sh scripts/boot-direct.sh --exclusive --preflight-only --mode full
```

## Donor reconciliation (DR-52)

No donor code was copied. The document compares how the donors isolate timing tests and how their
thread entry/wakeup works, to classify the stall.

| Topic | Illumos finding | BSD file/licence | Linux finding | Decision |
|---|---|---|---|---|
| SMP/timing test host isolation | no donor harness gate; uses fakes and dedicated CI (CDDL, behaviour only) | no permissive donor gate to port; the in-tree S107 `tests/smp/boot/host-idle.sh` is BSD-3 and used as-is | CI practice, read-only outcome | reuse the in-tree exclusive preflight; no copy |
| thread create/join stall classification | `lwp_create`/zombie state machine (CDDL, no copy) | NetBSD `36eddeb` `lib/libc/stdio/flockfile.c` and `lib/libpthread/pthread_create.c`; FreeBSD `13334440` `_flock_stub.c`/`pthread_create.c` (owner-tid + wakeup; behaviour only) | clone/futex wakeup (read-only) | classify only; point owners at the Raven spin wait and thread create; no donor code |

Donor revisions: `netbsd 36eddebcc36cb0775aa2962005b9c05419a6ff01`,
`freebsd 133344405f532bd54ff3629a77d4c4e062f01fdb`,
`illumos-gate 586678ac1bcbd77644a6ec4dac7f79d676cfb4ec`,
`musl 7813e9119eb5fe681b6750b8a033a0490bca0941`.
