<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# aarch64 user-SMP soak (S76c)

Status: OBSERVED on QEMU `virt`, `-smp 4`, `profile = base`, for the commit named below. Pi 4 is
UNVERIFIED.

## 1. What runs

`tests/fixtures/smp-soak-aarch64` starts eight user threads in one process (nine user threads with
the controller). The threads pass a futex token around a ring; the holder of the token performs one
work unit before waking the next thread:

* **FP/SIMD** — the worker parks a private tag in both lanes of `V0` immediately before blocking
  and re-checks it on resume, then runs a two-lane NEON add (`[tag, tag] + [1, 2]`) and verifies
  both lanes. A scheduler that loses the per-thread `Q0..Q31`/`FPSR`/`FPCR` area across a switch
  produces a wrong tag or a wrong sum.
* **Pipe traffic** — one byte through a shared non-blocking pipe per hop.
* **TLB churn** — every eighth hop the worker creates a memory object, maps two pages shared,
  writes and reads them back, then unmaps it (one remote TLB shootdown per unmap) and closes the
  object. A stale translation shows up as a wrong read-back.
* **Memory pressure** — the churn mapping above is the allocation/free pressure; the ring keeps it
  continuous for the whole run.

The controller measures the wake-to-run latency of every handoff, watches the hop counter for a
stalled ring (3 s without progress fails the run), stops the ring after the configured duration and
prints per-thread and per-CPU counts, hops, throughput and p50/p99 latency. `smp_user=off` uses the
same binary with the `off` entry point.

**Per-CPU numbers are inferred, not measured.** The kernel has no `getcpu` syscall. The fixture
uses the kernel's placement policy: with `smp_user=on` the first user thread (the controller) takes
CPU 0 and each later `THREAD_CREATE` advances the round-robin counter, so worker `index` runs on
`(index + 1) % 4`; `smp_user=off` pins every user thread to CPU 0. aarch64 user threads resume on
their `last_cpu` and are never stolen across CPUs, so the placement is stable for the run. The
inference is a policy statement; the measured facts are the per-thread counts, the hop total, the
churn count and the latency percentiles.

The ring guarantees a cross-CPU handoff in `on` mode: worker `i` and worker `i+1` always sit on
different CPUs, so every one of the hops below is a cross-CPU futex wake.

## 2. Measured tables

Both runs: 60 s, 8 workers, `-smp 4`, QEMU TCG on a shared host. One run each; the host is shared,
so absolute rates vary between runs — the first `on` run of the same build measured 21 480 hops
(358 hops/s, p99 3 812 688 ns) against the 43 371 below. Treat the rates as order-of-magnitude and
the per-CPU balance as the invariant.

| Metric | `smp_user=on` | `smp_user=off` |
|---|---|---|
| workers | 8 | 8 |
| duration (s) | 60 | 60 |
| hops | 43 371 | 49 188 |
| hops/s | 722 | 819 |
| map/unmap churn pairs | 5 416 | 6 144 |
| p50 wake-to-run (ns) | 510 592 | 451 504 |
| p99 wake-to-run (ns) | 977 360 | 1 003 056 |
| latency samples | 8 192 (capped) | 8 192 (capped) |
| per-CPU cpu0 | 10 842 | 49 188 |
| per-CPU cpu1 | 10 843 | 0 |
| per-CPU cpu2 | 10 843 | 0 |
| per-CPU cpu3 | 10 843 | 0 |
| FP/SIMD failures | 0 | 0 |
| stale-mapping failures | 0 | 0 |
| completion | 8/8 workers | 8/8 workers |
| process exit | `init exited (0)` | `init exited (0)` |

Reading the table: in `on` mode the ring is spread evenly over the four CPUs (the three application
processors each carry one quarter of the hops) and the run still completes with no FP/SIMD or
mapping failure. In `off` mode all work collapses onto CPU 0 as the policy requires. The single-CPU
ring is slightly faster in hops/s because every `on` hop is a cross-CPU wake through the reschedule
SGI; the p99 spread is the cost of that path, not a correctness difference.

## 3. Assertions the qtests enforce

`smp-soak-aarch64` fails unless: `smp: 4 cpus online`; all eight threads report a non-zero count;
all four per-CPU totals are non-zero; the hop total equals the sum of the thread counts; `churn` is
non-zero; the latency line has samples; every worker completes; and the process exits 0. The
case also forbids `panic`, `EXCEPTION`, `smp-soak-aarch64: failed`, `fp_bad` and `map_bad`.
`smp-soak-off-aarch64` is the same with `mode=off`, `cpu~0` for every thread and
`cpu1=cpu2=cpu3=0`.

The two failure paths were exercised before the final run: forcing a wrong FP lane expectation
produced `fp_bad=193` and `failed fp/simd corruption`; forcing a wrong read-back expectation
produced `map_bad=270` and `failed stale mapping`. Both failed the case through the forbidden
patterns, so neither assertion is a no-op.

The soak does **not** enable `smp-user-trace`: that trace prints one line per cross-CPU wake, which
would be tens of thousands of lines in 60 s and would dominate the measurement. Cross-CPU user
placement and wake evidence comes from `smp-user` (x86-64) and `smp-tlb-aarch64` (aarch64), which
assert the kernel's `smp-user: cpu N first user thread` line.

## 4. Reproduce

```
cargo xtask qtest --case smp-soak-aarch64 --arch aarch64
cargo xtask qtest --case smp-soak-off-aarch64 --arch aarch64
```

Both cases use `qemu_extra = ["-smp", "4"]`. The aarch64 kernel has no console input path, so the
fixture does not read its configuration from the serial line; the mode is compiled into the two
binaries and the duration is the `DURATION_S` constant in `src/bin/on.rs` / `src/bin/off.rs`.

## 5. Limits

* Pi 4 is UNVERIFIED: these are QEMU `virt` numbers with no board serial log.
* The host is shared and QEMU runs with TCG, so the latency percentiles include host scheduling
  noise; they are comparative, not real-hardware latencies.
* Per-CPU counts are inferred from the placement policy (see section 1); there is no per-CPU
  measurement syscall.
* The soak is test-only. It found no kernel regression in this build.
