# S176 clock and sleep conformance

SPDX-License-Identifier: BSD-3-Clause
Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

This directory is the table-driven S176 row for the S172 conformance harness. It builds one static
C probe against the Raven libc, stages it as a one-shot svcd service over the release initfs, and
runs it under QEMU. No product source is changed.

## Rows

- `clocks: ids ok` - `CLOCK_REALTIME=0`, `CLOCK_MONOTONIC=1`.
- `clocks: getres ok` - non-zero, consistent `clock_getres` for realtime and monotonic.
- `clocks: monotonic ok` - 10,000 samples are non-decreasing and never zero.
- `clocks: monotonic_smp ok threads=2` - two pthreads sample `CLOCK_MONOTONIC` on the `-smp 2`
  image; every sample across both threads is non-decreasing and non-zero. The qtest also asserts
  the kernel serial lines `smp: 2 cpus online` and `sched: running (2 cpus, slice ...)`; the libc
  `sysconf`/`sched_getcpu` stubs report 1/0, so CPU identity comes from the kernel lines.
- `clocks: sleep ok elapsed_ns=N` - `nanosleep` takes at least the 100 ms request.
- `clocks: eintr ok remaining_ns=N` - a handled SIGUSR1 interrupts a 1 s `nanosleep` with a
  positive unslept remainder and `EINTR`.
- `clocks: gettimeofday ok delta_ns=N` - wall-clock and `CLOCK_REALTIME` agree within 50 ms.
- `clocks: rates ok real_ns=N mono_ns=N` - both clocks advance about the 100 ms sleep.
- `clocks: ok` and `init exited (0)`.

`clocks-mutation.toml` builds the same probe with `-DMUTATION_EARLY_SLEEP` (a 1 ms sleep) and
expects the sleep row to fail. The mutation run is the negative check for the at-least assertion.

## Run

```sh
timeout 180 cargo xtask qtest --case tests/conformance/clocks/clocks.toml --arch x86_64
```

The normal row currently exposes the libc `nanosleep` EINTR mapping: the C wrapper returns 0 with a
full remaining value instead of `-1`/`EINTR`. That is a libc owner handover, not a test change.

## Donor map

- FreeBSD `sys/kern/kern_time.c` (`kern_clock_gettime`, `kern_gettimeofday`, `kern_nanosleep`),
  `sys/kern/kern_clock.c` (`hardclock`/`cpu_tick`), `sys/sys/time.h` clock ids; BSD-2-Clause.
- NetBSD `sys/kern/kern_time.c` (`clock_gettime1`, `gettimeofday1`, `nanosleep1`),
  `sys/kern/kern_clock.c`; BSD-2/3-Clause.
- Illumos `uts/common/os/clock.c` and `uts/common/os/time.c`: behaviour only.
- Linux `kernel/time/posix-timers.c` and `kernel/time/hrtimer.c`: outcome only, GPL.
- POSIX `clock_gettime(2)`, `clock_getres(2)`, `nanosleep(2)`, `gettimeofday(2)`.
