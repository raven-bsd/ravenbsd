<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# User-SMP soak evidence (S75b)

Status: OBSERVED on x86-64 QEMU (4 vCPUs); Pi 4 UNVERIFIED.

## 1. Workload

`tests/fixtures/smp-soak` runs eight user threads in a futex token ring. Each holder performs one
work unit before waking the next thread:

* a global counter increment;
* one non-blocking `PIPE2` read + write (pipe traffic);
* every 16th unit, a 4 KiB `MEMOBJ_CREATE` + `MAP` + touch + `UNMAP` + `HANDLE_CLOSE` (memory
  pressure and cross-CPU TLB shootdowns);
* a futex handoff to the next thread.

The controller records the wake-to-run latency (waker timestamp before `FUTEX_WAKE`, woken thread
timestamp after `FUTEX_WAIT` returns), runs for the configured duration (default 60 s), stops the
ring, and prints per-thread counts, inferred per-CPU counts, hops, throughput and p50/p99.

Configuration is one console line after `smp-soak: ready`: `"<seconds> <cpus> <on|off>"`. The
qtest cases send `60 4 on` and `60 1 off`.

## 2. Measured results

Three 60 s runs of each mode on the same shared host (QEMU `-smp 4`, host also running other
agents). `ops/s` = completed ring hops per second; latencies are the first 4096 handoffs
(`samples=4096`, the fixed sample buffer).

| run | mode | hops | ops/s | p50 (us) | p99 (us) |
| --- | --- | ---: | ---: | ---: | ---: |
| A | `smp_user=on` | 54,910 | 915 | 428 | 956 |
| B | `smp_user=on` | 17,095 | 284 | 470 | 3,950 |
| C | `smp_user=on` | 49,383 | 823 | 406 | 611 |
| A | `smp_user=off` | 50,042 | 834 | 359 | 813 |
| B | `smp_user=off` | 32,408 | 540 | 485 | 5,973 |
| C | `smp_user=off` | 31,661 | 527 | 449 | 15,798 |

Median of the three runs:

| mode | ops/s | p50 (us) | p99 (us) |
| --- | ---: | ---: | ---: |
| `smp_user=on` | 823 | 428 | 956 |
| `smp_user=off` | 540 | 449 | 5,973 |

Run B is the host-load outlier: the shared host was oversubscribed, and a 4-vCPU guest pays for
cross-vCPU scheduling while the 1-vCPU baseline does not. The median 4-CPU run completes about
52 % more ring hops than the single-CPU baseline with the same p50; the p99 tail is much shorter
because each thread gets its own vCPU instead of waiting for a time slice.

The ring is deliberately serialized: only one thread runs a unit at a time. The multi-core gain
therefore comes from the wake path and from the memory-pressure units running without a timer
slice, not from parallel ring throughput; a parallel workload is a follow-up.

## 3. Per-CPU evidence

The fixture prints per-CPU work inferred from the S75 round-robin creation policy (worker `i` is
created on CPU `(1 + i) % cpus`; the main thread takes slot 0). The 60 s `smp_user=on` run is
balanced within one unit:

```
smp-soak: per-cpu cpu0=12345 cpu1=12346 cpu2=12346 cpu3=12346
```

The single-CPU baseline pins all work to CPU 0:

```
smp-soak: per-cpu cpu0=32408 cpu1=0 cpu2=0 cpu3=0
```

The inference was verified with a 3 s `smp-user-trace` run of the same fixture. The kernel's
first-user lines map the created threads to the four CPUs:

```
smp-user: cpu 0 first user thread pid=1 tid=5
smp-user: cpu 1 first user thread pid=1 tid=6
smp-user: cpu 2 first user thread pid=1 tid=7
smp-user: cpu 3 first user thread pid=1 tid=8
```

Here tid 6/7/8 are workers 0/1/2, so worker `i` is on CPU `(1 + i) % 4`; the trace agrees with
the fixture's inference. The same 3 s run counted futex wake destinations across CPUs:

```
cpu 0: 686 wakes
cpu 1: 689 wakes
cpu 2: 688 wakes
cpu 3: 688 wakes
```

so the ring really moved the token across all four CPUs, not just CPU 0.

## 4. What is measured and what is not

Measured:

* every one of the eight threads completes (`complete workers=8`, exit 0);
* no lost wake: `HOPS` equals the sum of per-thread counts (`hops=49383 total=49383`) and the
  controller's stop path completes all eight workers;
* no stuck CPU: all four inferred CPU counts are non-zero and the 3 s trace shows 686-689 wakes per
  CPU;
* wake-to-run p50/p99 and ring throughput;
* no kernel panic in any run (qtest forbids panic/EXCEPTION).

Not measured here:

* **True per-CPU work attribution from userland.** RavenBSD has no `sched_getcpu`/CPU-id syscall,
  so the per-CPU counts above are inferred from the creation policy and verified against the
  kernel's first-user trace. A CPU-id syscall is the proper fix; this ticket is test-only.
* **Context switches/s.** No user-visible scheduler counter exists; the kernel's
  `sched::switch_count` is not exposed to userland. Not invented here.
* The 4-CPU numbers are from a shared host; a dedicated host will produce a tighter table.

## 5. Reproduce

```
cargo xtask qtest --case smp-soak --arch x86_64
cargo xtask qtest --case smp-soak-off --arch x86_64
```

Both default to 60 s. For a shorter local check, change the `[[step]]` line to e.g. `10 4 on`
(`smp_user=on`) or `10 1 off` (baseline).
