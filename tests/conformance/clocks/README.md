# S176 clock and sleep conformance

SPDX-License-Identifier: BSD-3-Clause
Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

This directory is the table-driven S176 row for the S172 conformance harness. It builds one
`no_std` Rust probe against the Raven libc, stages it as a one-shot svcd service over the release
initfs, and runs it under QEMU. No product source is changed.

## Rows

- `ids` - `CLOCK_REALTIME=0`, `CLOCK_MONOTONIC=1`.
- `getres` - non-zero, consistent `clock_getres` for monotonic and realtime.
- `monotonic` - 10,000 samples are non-decreasing and never zero.
- `monotonic-smp` - two pthreads each sample `CLOCK_MONOTONIC` 10,000 times on the qtest `-smp 4`
  image; every sample is non-decreasing and non-zero.
- `sleep` - `nanosleep` takes at least the 100 ms request.
- `eintr` - a handled SIGUSR1 interrupts a 1 s `nanosleep` with `-1`/`EINTR` and a positive unslept
  remainder smaller than the request.
- `gettimeofday` - wall-clock and `CLOCK_REALTIME` agree within 50 ms.
- `rates` - both clocks advance at least 50 ms over a 100 ms sleep and differ by at most 50 ms.

## Mutation

`mutations.tsv` names the controlled `early-sleep` mutation: a 1 ms request under the 100 ms
at-least assertion. Against the correct product the `sleep` row turns red, which is the negative
check for the at-least assertion.

## Run

```sh
python3 tests/conformance/harness/runner.py --topic tests/conformance/clocks --service clocks-conformance
python3 tests/conformance/harness/runner.py --topic tests/conformance/clocks --service clocks-conformance \
    --phase mutation --mutation early-sleep
```

## Donor map

- FreeBSD `sys/kern/kern_time.c` (`kern_clock_gettime`, `kern_clock_getres`, `kern_gettimeofday`,
  `kern_nanosleep`), `sys/kern/kern_clock.c` (`hardclock`, `cpu_tick`), `sys/sys/time.h` clock ids;
  BSD-2-Clause.
- NetBSD `sys/kern/kern_time.c` (`clock_gettime1`, `gettimeofday1`, `nanosleep1`),
  `sys/kern/kern_clock.c`; BSD-2/3-Clause.
- Illumos `usr/src/uts/common/os/clock.c` and `usr/src/uts/common/os/time.c`: behaviour only, CDDL.
- Linux `kernel/time/posix-timers.c` and `kernel/time/hrtimer.c`: outcome only, GPL.
- POSIX `clock_gettime(2)`, `clock_getres(2)`, `nanosleep(2)`, `gettimeofday(2)`.
