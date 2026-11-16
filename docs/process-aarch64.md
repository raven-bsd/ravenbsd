<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# aarch64 process/signal qtest parity (P2g)

Status: **PARTIAL.** The process/signal syscalls are common kernel code and the aarch64
target check passes. On the aarch64 qtest image, `proc-session` passes, but `proc-signal` and
`proc-fork` stop with a kernel data abort at the stack guard (`FAR=0xffffc0000000e000` /
`0xffffc0000000f000`, ELR in `__exception_vectors`) while entering the shared `PROC_FORK`/COW
path. The 20 KiB kernel stack is defined in `kernel/src/sched/thread.rs`
(`KERNEL_STACK_PAGES = 5`), outside P2g's Owns; this is a shared kernel-stack defect, not an
aarch64-only source path. No board claim.

## 1. Build

The P2f fixture build now selects the target from `RAVEN_QT_ARCH`:

```sh
RAVEN_QT_ARCH=aarch64 sh tests/fixtures/proc/build.sh
# x86_64 remains the default
sh tests/fixtures/proc/build.sh
```

The qtest harness exports `RAVEN_QT_ARCH` for the case `pre` command, so the same
`proc-signal`/`proc-session` case names build the matching `signal.img`/`session.img`.

## 2. Test matrix

| Case | aarch64 | Evidence |
|---|---|---|
| `proc-session` | **PASS** | groups, stop/continue, P_PGID/P_ALL waits, times/rusage and rlimits all printed |
| `proc-signal` | FAIL | kernel data abort at stack guard before the fixture prints |
| `proc-fork` | FAIL | kernel data abort at stack guard after the invalid-combo probes |
| `proc-handle-info` | FAIL (build) | fixture is x86_64-only; aarch64 boot rejects the ELF machine |
| `proc-exec` | absent | no case file on this base |

The aarch64 kernel check and the x86_64 proc cases remain green. `proc-session` is the positive
aarch64 parity result: the new shared wait/session/times/rlimit code runs on aarch64.

`proc-session` aarch64 serial:

```
proc-session: pgid=1 child-a=2 child-b=3
proc-session: setpgid=1 getpgid=1 sid-b=1
proc-session: stop=4991 cont=65535
proc-session: kill-a=35072 kill-b=35072
proc-session: times=1 rusage=1 rlimit=1 invalid=1 nproc=1 echild=1
proc-session: ok
init exited (0)
```

`proc-signal` aarch64 failure:

```
process: starting /init
sched: running (1 cpu, slice 10 ms)
exception: sync-cur-spx EC=0x25 (data abort same EL) ESR=0x0000000096000047
  ELR=0xffffffff80235240 FAR=0xffffc0000000e000 SPSR=0x00000000000003c5
exception: halted
```

`proc-fork` aarch64 failure (after `proc-fork: invalid combo=1 shared-zero=1 flags=1`):

```
exception: sync-cur-spx EC=0x25 (data abort same EL) ESR=0x0000000096000047
  ELR=0xffffffff80235240 FAR=0xffffc0000000f000 SPSR=0x00000000000003c5
exception: halted
```

## 3. Root cause and required follow-up

The fault address is the unmapped guard page immediately below the current kernel stack; the
saved frame pointer is at the bottom of the 20 KiB stack. The shared `PROC_FORK`/COW path uses
more than the aarch64 stack budget for these fixtures. `proc-session` uses one VMA per child and
does not cross the limit; `proc-signal` (control page plus fork/thread handoff) and `proc-fork`
(five mapped regions plus repeated fork) do.

The fix is either:

1. increase `KERNEL_STACK_PAGES` (currently five, 20 KiB, plus guard) in
   `kernel/src/sched/thread.rs`, which is outside P2g Owns; or
2. reduce the shared fork/COW stack footprint, likewise outside the aarch64/platform-only Owns.

A P2g rework needs an Owns extension for the shared scheduler stack, or a follow-up ticket. The
aarch64-specific `TrapFrame`/signal context is not the cause: `proc-session` exercises the
common syscall/trap path successfully; the failure is only on the deeper fork fixtures.

`proc-handle-info` needs an aarch64 fixture build (its `tests/fixtures/proc-handle-info/build.sh`
is x86_64-only and outside P2g Owns). `proc-exec` does not exist on this base.

## 4. Provenance

No donor code was read or copied. The fixture build change and this document are first-party
BSD-3-Clause work.
