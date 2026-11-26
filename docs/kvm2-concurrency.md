<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# KVM2 `smp-libc-concurrency` investigation

Status: **BLOCKED on reproduction/classification.** No kernel fix is claimed. The closeout cell at
`d79d7679` timed out after `smp-libc-concurrency: start`; this clone could not reproduce that
specific stop, and the qtest/QEMU process-exit hangs below confounded repeated runs.

## Closeout evidence (preserved in the S136 ticket)

- KVM2 cell: `smp-libc-concurrency-kvm2`, timeout, `duration_ms=29857`.
- Serial stops at:

```
smp-libc-concurrency: start
```

- Missing: `malloc ok`, `stdio ok lines=16000`, `done`, `svcd: settled`, `init exited (0)`.
- KVM1 passes in 14.5 s; TCG4 in 38.1 s. Host load 1.2-1.3 with only the cell QEMU.

The first operation after `start` is `fopen("/tmp/smp-libc-concurrency.log", "w")`; the fixture
then calls `pthread_create` eight times and joins. A hang at `start` can therefore be the `/tmp`
scheme open (vfsd/ramfsd) *or* thread creation/join; the closeout line alone does not distinguish
them.

## Reproduction attempts in this clone

- Clean image, exact bounded run:
  `timeout 30 cargo xtask qtest --case smp-libc-concurrency-kvm2 --arch x86_64 --no-build`
  passed in 8.2 s with every expected marker and `init exited (0)`.
- A repeated run caught a different failure: the guest log was **complete through
  `init exited (0)`**, but qtest timed out at 30 s because QEMU did not exit. Preserved at
  `/tmp/opencode/s136-fail-2.log`.
- After such a timeout, a QEMU process can remain holding
  `build/qtest/smp-libc-concurrency-kvm2.img`; subsequent runs then fail immediately with an
  empty serial log / `crash`, not the closeout's `start` hang. Killing the QEMU and rebuilding the
  image restores a passing run.
- The old `build/qtest/smp-libc-concurrency-kvm2.json` is stale and was not used as evidence.

## Classification

- **Kernel path:** no kernel panic, exception, TLB timeout, scheduler/IPI diagnostic or `svcd`
  readiness failure appeared in the reproduced pass. The closeout failure has no kernel trace
  naming a kernel path.
- **Guest path:** the stop is after `start`, before `malloc ok`; either `/tmp` open or the libc
  `pthread_create`/join/list path is the first suspect.
- **Harness path:** the independently reproduced `init exited (0)` + qtest timeout with a live
  QEMU is a QEMU/qtest exit hang. It also causes image-lock residue that mimics a crash on the
  next run.

## Handoff

- **Tim / libc thread toolchain:** investigate the `fopen`/`pthread_create`/join path and the
  thread list under KVM2; the closeout log is in the S136 ticket and the passing trace is
  `/tmp/opencode/s136-final-exact.log` (when the clean image run passes).
- **Phil / Bob / qtest:** investigate why QEMU remains alive after a complete guest run; the
  preserved full log is `/tmp/opencode/s136-fail-2.log`, and the image lock path is
  `build/qtest/smp-libc-concurrency-kvm2.img`.

No timeout was raised, no test was weakened, and no retry was used to claim a pass.
