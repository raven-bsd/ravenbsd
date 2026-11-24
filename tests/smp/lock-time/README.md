# S85a SMP lock/time/IPI stress

<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

The guest fixture (`src/main.rs`) runs eight futex-ring workers and up to four per-CPU timer
threads on four CPUs:

* **turnstile / futex contention** — eight workers pass a token around a ring; every handoff is
  a futex wait/wake through the kernel sleeping-lock path and a cross-CPU reschedule IPI.
* **per-CPU timer cadence** — four timer threads call `nanosleep(1 ms)` continuously and record
  the wake jitter, exercising the per-CPU cyclic timers and timer IPIs.
* **preemption churn** — eight workers and four timers compete for four CPUs; each worker calls
  `yield_now` every 64 hops and, when enabled, creates/maps/unmaps a memory object every
  `mem_period` hops (the TLB shootdown stress).
* **iterations** — the controller runs the requested number of 100 ms iterations, prints
  wake-to-run and timer-jitter p50/p90/p99/max per iteration, and prints the worst p90/max at the
  end. A zero-progress iteration, a wake stall (>2 s) or a timer stall (>1 s) fails the run; no
  retry is attempted.

The controller config line is:

```
<iterations> <cpus> <on|off> <mem_period> <timers>
```

`mem_period=0` disables the map/unmap TLB pressure; `timers=0` disables the timer threads. The
defaults are `50 4 on 16 4`.

## Build and run

```sh
sh tests/smp/lock-time/build.sh
cargo xtask qtest --case tests/smp/lock-time/lock-time-tcg.toml --arch x86_64
sh tests/smp/lock-time/run.sh tcg      # TCG -smp 4, 50 iterations, keeps the serial log
sh tests/smp/lock-time/run.sh kvm      # KVM -smp 4 and -smp 2, after S83
sh tests/smp/lock-time/run.sh nomem    # TCG, TLB pressure off (lock/timer control)
sh tests/smp/lock-time/run.sh smp2     # TCG -smp 2, same TLB/timer workload
sh tests/smp/lock-time/run.sh repro    # 10x memory-only reproducer
sh tests/smp/lock-time/run.sh variants # nomem + smp2 + repro
sh tests/smp/lock-time/run.sh all
```

The qtest harness defaults to `-smp 4` for x86_64. The KVM cases append `-accel kvm -cpu host`
(and `-smp 2` for the two-CPU case). KVM `-smp 2/4` is gated on S83; if it fails or hangs, the
runner exits non-zero and the serial log is kept under `build/smp/lock-time/logs/`. A flake or a
lost wake is a blocker, not a retry.

## TLB narrowing variants

`lock-time-tcg-nomem.toml` disables the TLB pressure (`mem_period=0`) and keeps the timers and
futex ring: it passes 50/50, so the lock/timer/IPI path is clean without map/unmap.

`lock-time-tlb-repro.toml` is the former deterministic reproducer for
`kernel/src/smp/shootdown.rs:122`: memory pressure on every hop (`mem_period=1`), no timers
(`timers=0`), 10 iterations. S113 fixed target IPI delivery (IPC and user page-fault
interruptibility); the former `smp: tlb shootdown timeout on cpu N` line is now forbidden, so the
case is a strict clean gate.

`lock-time-tcg2.toml` runs the full TLB+timer workload on TCG `-smp 2`; the guest completes
50/50 with no timeout. It is now a strict clean gate like `lock-time-tcg.toml` (4 CPUs).

The 50/50 TCG and KVM `-smp 2/4` gate is green for S113; see `tickets/stuart/reports/S113.md`.

Expected tail for a clean run:

```
lock-time: summary iterations=50 hops=... worst_p90_ns=... worst_max_ns=... worst_timer_p90_ns=... worst_timer_max_ns=... mem_period=...
lock-time: complete iterations=50 workers=8 timers=4
lock-time: done
init exited (0)
```
