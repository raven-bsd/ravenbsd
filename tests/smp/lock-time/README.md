# S85a SMP lock/time/IPI stress

<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

The guest fixture (`src/main.rs`) runs eight futex-ring workers and four per-CPU timer threads on
four CPUs:

* **turnstile / futex contention** — eight workers pass a token around a ring; every handoff is
  a futex wait/wake through the kernel sleeping-lock path and a cross-CPU reschedule IPI.
* **per-CPU timer cadence** — four timer threads call `nanosleep(1 ms)` continuously and record
  the wake jitter, exercising the per-CPU cyclic timers and timer IPIs.
* **preemption churn** — eight workers and four timers compete for four CPUs; each worker calls
  `yield_now` every 64 hops and periodically creates/maps/unmaps a memory object.
* **iterations** — the controller runs **50 iterations of 100 ms** (5 s guest time), prints
  wake-to-run and timer-jitter p50/p90/p99/max per iteration, and prints the worst p90/max at the
  end. A zero-progress iteration, a wake stall (>2 s) or a timer stall (>1 s) fails the run; no
  retry is attempted.

## Build and run

```sh
sh tests/smp/lock-time/build.sh
cargo xtask qtest --case tests/smp/lock-time/lock-time-tcg.toml --arch x86_64
sh tests/smp/lock-time/run.sh tcg      # TCG -smp 4, 50 iterations, keeps the serial log
sh tests/smp/lock-time/run.sh kvm      # KVM -smp 4 and -smp 2, after S83
sh tests/smp/lock-time/run.sh all
```

The qtest harness defaults to `-smp 4` for x86_64. The KVM cases append `-accel kvm -cpu host`
(and `-smp 2` for the two-CPU case). KVM `-smp 2/4` is gated on S83; if it fails or hangs, the
runner exits non-zero and the serial log is kept under `build/smp/lock-time/logs/`. A flake or a
lost wake is a blocker, not a retry.

Expected tail:

```
lock-time: summary iterations=50 hops=... worst_p90_ns=... worst_max_ns=... worst_timer_p90_ns=... worst_timer_max_ns=...
lock-time: complete iterations=50 workers=8 timers=4
lock-time: done
init exited (0)
```
