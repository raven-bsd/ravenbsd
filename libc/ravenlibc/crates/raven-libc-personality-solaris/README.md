<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# raven-libc-personality-solaris

The ring-2 Solaris/illumos personality of ravenlibc (ticket C67, `02-api-surface` S01, S02 and
the `thr_*`/`synch_*` spellings, `17-crate-layout` row 34, `14` 3.B.3.e). The crate exists and
compiles in every build; the C symbols are exported only when the shell feature
`personality-solaris` is on (the `server` bundle; off in `base` and `tiny`).

The crate is pure logic: no pointer, no syscall, no clock and no filesystem access of its own.
Every kernel or server interaction is a narrow trait in `seams`; the C-ABI shell
(`raven-libc/src/personality_solaris/**`) implements those traits over `raven-libc-platform`,
`raven-libc-sys` and `raven-libc-thread`, and the host tests implement them with the
deterministic fakes in `testutil`.

## Facilities

| Module | API | Status |
|---|---|---|
| `port` | `port_create/associate/dissociate/get/getn/send` over EVQ and a user-event doorbell | complete over the seams; a port is one EVQ handle plus one doorbell, associations are one-shot, cookies are `(fd << 32) | epoch` |
| `port::alert` | `port_alert` | exported, always `ENOSYS` (doc 06 gives no semantics; C67 12.3 QUESTION) |
| `identity` | `getexecname` from `AT_EXECFN`, `getisax` from `AT_HWCAP`/`AT_HWCAP2` | complete; never a file, never `/proc` (A-sol-4, `PG-50`, `PG-51`) |
| `thread_names` | `thr_*`, `mutex_*`, `cond_*`, `rwlock_*`/`rw_*`, `sema_*` over `PosixThreads` | the wrappers and translations are complete; the shell calls the merged C57 crate and returns its kernel-gap errors (`PG-2`, `PG-4`, `PG-7`) |
| `consts` | `PORT_SOURCE_*`, `THR_*`, `USYNC_*`, `PortEvent` | one file, every value labelled (`doc 14` for the two public facts, `ours` for the rest) |

## Clean room

`illumos-gate` is CDDL-1.0 and was read for behaviour only. No line, comment, table, macro body or
constant was copied or translated, and no illumos identifier appears in the crate or its headers.
`06-solaris-design.md` is the design source; the constants that the design does not state as
public facts are chosen here and documented as ours.

## C57 is merged

`raven-libc-thread` is present and every pthread/semaphore-backed export calls it:

* `seams::PosixThreads` is implemented by the recording fake in the host tests and by the shell
  over `raven-libc-thread::{thread,mutex,cond,rwlock,sem,tsd}` plus C58's `pthread_sigmask`.
* No export is marked `deferred`; the wrappers return the C57 seam's own error when a kernel
  primitive is absent. Today that is `ENOSYS` for `thr_create` (`PG-2`) and `thr_kill` (`PG-7`);
  process-shared `USYNC_PROCESS` locks are `ENOTSUP` (`PG-4`).
* The wrappers that need no thread runtime remain real: `thr_min_stack` (the constant),
  `thr_main`/`thr_self` on the only thread, `thr_yield` (the kernel yield), and the process-wide
  `thr_setconcurrency`/`thr_getconcurrency` hint.
* `thr_suspend`/`thr_continue` return `ENOSYS` for `PG-48`.

## Deviations recorded by C67

* `__SOLARIS_VISIBLE` is not defined by C37's `sys/_visible.h`; the three headers that need it
  define it as `__BSD_VISIBLE` (on by default, cleared in strict modes) until C37 adds it.
* The ticket's 6.9 list has 50 names, not 51; all 50 are exported.
* `raven-libc-sys` is a declared dependency (the `crates.toml` row) but the crate uses
  `raven-libc-abi`/`raven-libc-platform` directly; the shell implements `PosixThreads` over
  `raven-libc-thread` and C58's `pthread_sigmask`.

## Tests

`cargo test -p raven-libc-personality-solaris` runs the `_ut.rs` tables (constants and timeout
conversion, seam shapes, `getexecname`/`getisax`, every port rule and 5.D scenario, every thread
wrapper and flag rule) plus the integration tests:

* `tests/header_parity.rs` parses `include/port.h`, `include/thread.h` and `include/synch.h` and
  asserts equality with `consts.rs`, the typedefs and the visibility guard.
* `tests/thread_deferred.rs` reads `exports.toml`, asserts that no export is deferred and that the
  50-name table contains every thread-runtime spelling.
* `tests/emptiness.rs` checks a built shell archive for the presence or absence of the 50 names
  and for the absence of every `door_*`/`kstat_*` symbol (SKIP when no archive exists).
