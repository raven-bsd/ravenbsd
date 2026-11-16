<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# aarch64 process/signal qtest parity (P2g)

Status: **PARTIAL.** The process/signal syscalls are common kernel code and the aarch64
target check passes. On the aarch64 qtest image, `proc-session` and `proc-handle-info` pass,
but `proc-signal` and `proc-fork` stop with a kernel data abort at the stack guard
(`FAR=0xffffc0000000e000` / `0xffffc0000000f000`, ELR in `__exception_vectors`) while entering
the shared `PROC_FORK`/COW path. The 20 KiB kernel stack is defined in
`kernel/src/sched/thread.rs` (`KERNEL_STACK_PAGES = 5`), outside P2g's Owns; this is a shared
kernel-stack defect, not an aarch64-only source path. No board claim.

## 1. Build

The P2f and P9b fixture builds select the target from `RAVEN_QT_ARCH`:

```sh
RAVEN_QT_ARCH=aarch64 sh tests/fixtures/proc/build.sh
RAVEN_QT_ARCH=aarch64 sh tests/fixtures/proc-handle-info/build.sh
# x86_64 remains the default
sh tests/fixtures/proc/build.sh
sh tests/fixtures/proc-handle-info/build.sh
```

The qtest harness exports `RAVEN_QT_ARCH` for the case `pre` command, so the same
`proc-signal`/`proc-session`/`proc-handle-info` case names build the matching initfs images.
The handle-info fixture also has an aarch64 `svc #0` syscall stub alongside the x86-64 `syscall`
stub.

## 2. Test matrix

| Case | aarch64 | Evidence |
|---|---|---|
| `proc-session` | **PASS** | groups, stop/continue, P_PGID/P_ALL waits, times/rusage and rlimits all printed |
| `proc-handle-info` | **PASS** | pid/info, wait status, post-exit info, stale and non-process EBADF all printed |
| `proc-signal` | FAIL | kernel data abort at stack guard before the fixture prints |
| `proc-fork` | FAIL | kernel data abort at stack guard after the invalid-combo probes |
| `proc-exec` | absent | no case file on this base; P2e (Stuart) follow-up |

The aarch64 kernel check and the x86_64 proc cases remain green. `proc-session` and
`proc-handle-info` are the positive aarch64 parity results: the shared wait/session/times/rlimit
code and the process-handle-info path run on aarch64.

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

`proc-handle-info` aarch64 serial:

```
proc-handle-info: pid=2 info=ok
proc-handle-info: wait=42 status=10752
proc-handle-info: exited info=ok
proc-handle-info: stale=badf
proc-handle-info: nonprocess=badf
proc-handle-info: ok
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

Both failures are the same shared kernel-stack exhaustion in the `PROC_FORK`/COW path, not an
aarch64-only source path. The kernel-stack allocation is six pages: the lowest page is unmapped
as the guard, and the five pages above it (20 KiB) are the valid stack.

* `proc-signal`: guard `0xffffc0000000e000`, valid stack starts `0xffffc0000000f000`, and the
  interrupted kernel `SP` was `0xffffc0000000e0a0` — 0xf60 (3936) bytes below the valid stack
  bottom. The synchronous exception entry subtracts 288 bytes and stores at `SP+128`, which is
  the guard base, so the trap-frame store faults (`ELR` is the `stp x16,x17` in vector slot 4).
* `proc-fork`: the same pattern with guard `0xffffc0000000f000` and
  `SP=0xffffc0000000f0a0`.
* The saved frame pointer (`x29`) is `guard+0x1040`, 64 bytes above the stack bottom. The gap
  from that frame to the interrupted `SP` is 4000 bytes, so the overflow is a deep multi-frame
  chain, not the 288-byte trap frame alone.
* The aarch64 build has several ~4 KiB frames in this path: `process::build_stack` 4000 bytes,
  `process::load_segments` 3984, `process::teardown` 3968, and `fork_clone_from` 2928 (the
  x86-64 `fork_clone_from` prologue uses 2464).

The aarch64 chain therefore needs at least 20,480 + 3,936 = 24,416 bytes before the 288-byte
exception frame, i.e. over 24.7 KiB. A six-page (24 KiB) stack would still be short; seven pages
(28 KiB) restore the ~4 KiB headroom the five-page budget was sized for. The fix is outside
P2g's Owns:

1. raise `KERNEL_STACK_PAGES` (currently five, 20 KiB, plus guard) in
   `kernel/src/sched/thread.rs`; or
2. reduce the shared fork/COW stack footprint (the large aarch64 frames above), likewise
   outside the aarch64/platform-only Owns.

A P2g rework needs an Owns extension for the shared scheduler stack, or a follow-up ticket. The
aarch64 `TrapFrame`/signal context is not the cause: `proc-session` and `proc-handle-info`
exercise the common syscall/trap path successfully; the failure is only on the deeper fork
fixtures.

`proc-exec` has no case file on this base; it is a P2e (Stuart) follow-up. `procd` was not
edited.

## 4. Provenance

No donor code was read or copied. The fixture build change and this document are first-party
BSD-3-Clause work.
