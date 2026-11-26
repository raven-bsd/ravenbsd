<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# Time: monotonic clock, wall clock and the RTC

Status: OBSERVED for the monotonic clock (S04/P2); PROPOSED for the wall clock (S72).

## Clocks

| Clock | Source | Notes |
|---|---|---|
| `CLOCK_MONOTONIC` | TSC (x86-64) / `CNTPCT_EL0` (aarch64), scaled by `raven-timemath` | never adjusted, only advances |
| `CLOCK_REALTIME` | platform RTC at boot, plus monotonic elapsed | settable via `CLOCK_SET` |

`CLOCK_REALTIME` returns the RTC epoch plus the monotonic elapsed time since boot, so it keeps
advancing across suspends of the RTC. `boot_realtime_ns()` reads the RTC lazily on first use and
prints a one-line transcript (`rtc: YYYY-MM-DD HH:MM:SS (epoch Ns)`); a board with no usable RTC
prints `rtc: unavailable` and `CLOCK_REALTIME` stays at zero (boot never blocks on the RTC).

## RTC drivers

* **x86-64** (`kernel/src/arch/x86_64/rtc.rs`): CMOS/MC146818A through ports `0x70`/`0x71`. The
  driver waits out the update-in-progress window, decodes BCD or binary per status register B
  (including 12/24-hour mode), and converts the date to an epoch with Hinnant civil-date
  arithmetic. The century is not in the CMOS chip: the year register maps to `2000 + year`
  (valid 2000-2099), matching QEMU and a modern BIOS.
* **aarch64** (`kernel/src/arch/aarch64/rtc.rs`): PL031 on QEMU `virt` (`0x0901_0000`), read
  through the HHDM after mapping the page with device attributes on first use. The PL031 data
  register is already epoch seconds.

## Syscalls

* `CLOCK_GET` (`0x04`) returns `Timespec` for `REALTIME` (advancing) and `MONOTONIC`.
* `CLOCK_SET` (`0x09`, reviewer-assigned; not yet a `raven-abi` variant): sets `REALTIME` to a
  user `Timespec`; `MONOTONIC` is `EINVAL`, out-of-range values are `EINVAL`.

## Interval timers (`setitimer`/`getitimer`)

`ITIMER_REAL` is one process timer shared by `alarm`, `ualarm`, `setitimer` and `getitimer`:

* the deadline/interval arithmetic is `raven-libc-time::itimer::ITimer` (selector and `timeval`
  range validation, the previous value, zero-value cancellation and interval-aware remaining);
* the shell owns one kernel timer (`TIMER_CREATE`/`TIMER_SET`) and one detached thread that waits
  on its event queue and queues `SIGALRM` through `PROC_SIGNAL`;
* `setitimer`/`getitimer` accept `ITIMER_REAL` and return `EINVAL` for `ITIMER_VIRTUAL`/`ITIMER_PROF`,
  reject negative or out-of-range `timeval` fields, and write the previous value to `old`;
* `alarm(2)` is the same state with a zero interval, so `alarm` and `setitimer` observe each
  other's remaining time.

The delivery seam is `pause`/`PROC_SIGRECV`: the queued `SIGALRM` interrupts `pause` with
`EINTR`. The `sigaction` disposition is stored, but the current seam consumes the queued signal
rather than dispatching the handler; that is pre-existing behaviour shared with `alarm(2)`
(`posix-alarm`). **OBSERVED** by the `itimer` qtest.
