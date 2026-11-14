<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# raven-libc-personality-linux

The ring-2 Linux/GNU personality of ravenlibc (ticket C66 5.A, `02-api-surface` L01-L05, L09,
X54, X73, X74, X85, X86 and the `*64` aliases).

The crate is pure logic: no pointer, no syscall, no clock and no filesystem access of its own.
Every kernel or server interaction is a narrow trait in `seams`; the C-ABI shell
(`raven-libc/src/personality_linux/**`) implements those traits over `raven-libc-sys` and
`raven-libc-platform`, and the host tests implement them with the deterministic fakes in
`testutil`.

## Facilities

| Module | API | Gap |
|---|---|---|
| `epoll` | `epoll_create/create1/ctl/wait/pwait/pwait2` over EVQ | `PG-25` for non-file readiness |
| `eventfd` | 64-bit counter, `EFD_SEMAPHORE` | `PG-19` for a blocking empty read |
| `timerfd` | create/settime, relative arms | `PG-17`/`PG-19` for absolute and `gettime` |
| `signalfd` | 128-byte `signalfd_siginfo` | `PG-7` (returns `ENOSYS`) |
| `inotify` | flags and masks validated first | `PG-38` (returns `ENOSYS`) |
| `prctl` | the native subset of `10` 3.8 | `procd` (`PG-44`) for the title |
| `personality` | `PER_LINUX` only | none |
| `error` | GNU `error()`/`error_at_line()` formatting | none (shell prints and exits) |
| `sysinfo` | Linux LP64 `struct sysinfo` | `PG-21` (`ENOSYS`, `get_nprocs*` = 1) |
| `sendfile` | 64 KiB bounce loop, offsets, clamping | `PG-22` for the offset form |
| `auxval` | `getauxval` present/absent rule | C61 retains only `AT_PAGESZ`, `AT_CLKTCK`, `AT_RANDOM`, `AT_SECURE` |
| `alias64` | the 23-row seed of C66 5.F | pending rows have no export |

## Deviations recorded by C66

* `raven-libc-signal` (C58) is not on this branch: the local `SigSet = [u64; 2]` and the
  `SignalOps` default (`ENOSYS`) replace it. `epoll_pwait` with a non-NULL mask is `ENOSYS`.
* The seams gain a few default methods (`FdTable::close_fd`/`set_cloexec`/`close_range_between`,
  the numeric `ProcCtl` state, `EventOps`/`TimerOps`/`SignalOps` getters, `FsWatch`). The C66
  4.2 method set is kept unchanged; the additions have honest defaults.
* `sysinfo::fill` has no clock seam: `uptime` is `boot_time_ns / 1e9` (the SYSINFO record's
  boot epoch), recorded in the C66 report.
* `copy_file_range` answers `EINVAL` for every same-descriptor call: without a cursor view the
  non-overlap case cannot be proven (the ticket's only required same-fd answer is the overlap
  `EINVAL`).
* There is no `Cargo.toml` dependency on `raven-libc-signal` (absent on this branch); every
  other dependency is exactly the `crates.toml` row of `17` 4.1.

## Tests

`cargo test -p raven-libc-personality-linux` runs the `_ut.rs` tables (epoll translation and
lifecycle, eventfd/timerfd, signalfd layout from the installed header, inotify masks, prctl,
personality, error goldens, sysinfo layout, sendfile sizes and errors, auxval, aliases) plus
the integration smoke test under `tests/`.
