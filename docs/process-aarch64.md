<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# aarch64 process/signal qtest parity (P2g)

Status: **PASS on the four available cases.** `proc-handle-info`, `proc-session`, `proc-signal`
and `proc-fork` pass on aarch64 after P2i raised the shared kernel stack to seven pages
(`KERNEL_STACK_PAGES = 7`, 28 KiB) in `kernel/src/sched/thread.rs`. The x86_64 controls pass the
same four cases. `proc-exec` is not on `origin/master`: it is the P2k same-pid `execve` work and
is reported as a P2k dependency, not skipped or faked. No board claim.

## 1. Build

The P2f/P9b fixtures select the target from `RAVEN_QT_ARCH`:

```sh
RAVEN_QT_ARCH=aarch64 sh tests/fixtures/proc/build.sh
RAVEN_QT_ARCH=aarch64 sh tests/fixtures/proc-handle-info/build.sh
# x86_64 remains the default
sh tests/fixtures/proc/build.sh
sh tests/fixtures/proc-handle-info/build.sh
```

The qtest harness exports `RAVEN_QT_ARCH` for the case `pre` command, so the same
`proc-handle-info`/`proc-session`/`proc-signal` case names build the matching initfs images. The
handle-info fixture has an aarch64 `svc #0` syscall stub beside the x86-64 `syscall` stub.

## 2. Test matrix

| Case | aarch64 | Evidence |
|---|---|---|
| `proc-handle-info` | **PASS** | pid/info, wait status, post-exit info, stale and non-process EBADF |
| `proc-session` | **PASS** | groups, stop/continue, P_PGID/P_ALL waits, times/rusage, rlimits |
| `proc-signal` | **PASS** | two queued signals, wait status, SIGCHLD sender, EAGAIN/invalid/unknown |
| `proc-fork` | **PASS** | COW data, NOFORK, ZERO_ON_FORK, object handle, four-cycle fork loop |
| `proc-exec` | P2k dependency | P2k same-pid `execve` is not on `origin/master`; no case or fixture exists yet |

The x86_64 controls pass the same four cases. The shared syscall numbers and the common
wait/session/times/rlimit path are unchanged.

### aarch64 serials

`proc-handle-info`:

```
proc-handle-info: pid=2 info=ok
proc-handle-info: wait=42 status=10752
proc-handle-info: exited info=ok
proc-handle-info: stale=badf
proc-handle-info: nonprocess=badf
proc-handle-info: ok
init exited (0)
```

`proc-session`:

```
proc-session: pgid=1 child-a=2 child-b=3
proc-session: setpgid=1 getpgid=1 sid-b=1
proc-session: stop=4991 cont=65535
proc-session: kill-a=35072 kill-b=35072
proc-session: times=1 rusage=1 rlimit=1 invalid=1 nproc=1 echild=1
proc-session: ok
init exited (0)
```

`proc-signal`:

```
proc-signal: rx1=10 rx2=12
proc-signal: wait pid=2 status=0
proc-signal: sigchld=17 sender=2
proc-signal: eagain=1 invalid=1 unknown=1
proc-signal: ok
init exited (0)
```

`proc-fork`:

```
proc-fork: child code=0
proc-fork: data parent=0x33334444 child-saw=1 child-wrote=0x55556666
proc-fork: nofork map=1 parent=0x11112222
proc-fork: zero child-ok=1 parent=0x77778888
proc-fork: nofork-object handle-absent=1 map-absent=1 parent=0x11112222
proc-fork: loop done cycles=4 counter=4
proc-fork: done
init exited (0)
```

## 3. P2i kernel stack

P2i merged the shared stack increase: `KERNEL_STACK_PAGES = 7` (28 KiB) plus one guard page in
`kernel/src/sched/thread.rs`. The old 20 KiB stack exhausted in the `PROC_FORK`/COW path and
faulted in the aarch64 exception vector; with seven pages the `proc-signal` and `proc-fork`
transcripts above complete normally. The previous 20 KiB analysis is retained in the P2g report
for history.

## 4. P2k dependency

`proc-exec` is owned by P2k (same-pid `execve`, `kernel/src/process/**`, `kernel/src/syscall/**`,
`tests/qtest/cases/proc-exec*`, `tests/fixtures/proc/**`). It is not on `origin/master` as of this
P2g round, so the case is not present and is not skipped here; the four available aarch64 cases
are run and this row is reported as the exact P2k dependency. `procd` was not edited.

## 5. Provenance

No donor code was read or copied. The fixture build changes, the aarch64 process tests and this
document are first-party BSD-3-Clause work.
