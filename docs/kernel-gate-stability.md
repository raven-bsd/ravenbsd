<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# Kernel gate stability (S80b)

Status: OBSERVED on QEMU x86_64 and aarch64; Raspberry Pi 4 UNVERIFIED.

Commit under test: `3d85baf553b8aeeeef5d9e3d614a5c8b8ef96b36` (`origin/master`).

Method: `sh scripts/kernel-gate.sh` was run three times, sequentially, in the foreground, from
the same clean checkout. Wall time was measured with `/usr/bin/time -p`. Each run executes the
27 fixed cases (22 x86_64, 5 aarch64) and writes `docs/kernel-gate.md`; the per-case serial logs
are under `target/kernel-gate/`. The generated `docs/kernel-gate.md` is deliberately not part of
this change, so the gate's own output stays the record of the last invocation.

## Three runs

| Run | Started (UTC) | Finished (UTC) | Wall time | Cases | Result | Timeouts |
| --- | --- | --- | ---: | ---: | --- | ---: |
| 1 | 2026-11-17T10:42:44Z | 2026-11-17T10:54:16Z | 691.84 s | 26/27 | **FAIL** (1 case) | 0 |
| 2 | 2026-11-17T11:00:44Z | 2026-11-17T11:11:08Z | 624.33 s | 27/27 | **PASS** | 0 |
| 3 | 2026-11-17T11:11:11Z | 2026-11-17T11:23:53Z | 762.08 s | 27/27 | **PASS** | 0 |

Run 1 is a real finding and is not averaged away: 2 of 3 runs are clean, so the gate is **not**
repeatable yet on this host.

## Failure by owner

### stuart — `kernel-timer` (x86_64)

* Result in run 1: **FAIL**, exit 35, no timeout.
* Serial: `selftest: timer 100 ticks in 163 ms` followed by
  `selftest: timer interval out of range`.
* The qtest expects `^selftest: timer 100 ticks in (9[0-9]|1[0-1][0-9]) ms$` (90-119 ms); the
  kernel's own selftest accepts 70-130 ms and also rejected 163 ms.
* The other 26 cases passed in the same run.
* The host is shared with other minions: at the start of run 2 the 1-minute load average was
  19.74 (5-minute 19.18, 15-minute 22.81) with other QEMU gates running, so this is a
  timing-sensitive failure. It is still a gate failure: the ticket's contract is 27/27 in every
  run, and a slow run must not be retried away.
* Suggested owner: stuart (timer selftest/measurement), as recorded by the gate table.

## Per-case results

`pass`/`FAIL` are the gate's own verdicts; the failure detail is in the section above.

| Case | Arch | Owner | Run 1 | Run 2 | Run 3 |
| --- | --- | --- | --- | --- | --- |
| boot-uname | x86_64 | stuart | pass | pass | pass |
| kernel-early-boot | x86_64 | stuart | pass | pass | pass |
| exc-int3 | x86_64 | stuart | pass | pass | pass |
| exc-div0 | x86_64 | stuart | pass | pass | pass |
| exc-pagefault | x86_64 | stuart | pass | pass | pass |
| exc-doublefault | x86_64 | stuart | pass | pass | pass |
| kernel-paging | x86_64 | stuart | pass | pass | pass |
| kernel-timer | x86_64 | stuart | **FAIL** | pass | pass |
| time-entropy | x86_64 | lance | pass | pass | pass |
| time-entropy-cyclic | x86_64 | lance | pass | pass | pass |
| smp-user | x86_64 | stuart | pass | pass | pass |
| smp-user-off | x86_64 | stuart | pass | pass | pass |
| irq-dist | x86_64 | stuart | pass | pass | pass |
| irq-dist-off | x86_64 | stuart | pass | pass | pass |
| locking | x86_64 | lance | pass | pass | pass |
| lockstat | x86_64 | lance | pass | pass | pass |
| ipc-donation | x86_64 | lance | pass | pass | pass |
| kmem-frames | x86_64 | mark | pass | pass | pass |
| kmem-frames-nopcpu | x86_64 | mark | pass | pass | pass |
| proc-death-cleanup | x86_64 | stuart | pass | pass | pass |
| trace-context | x86_64 | tim | pass | pass | pass |
| release-login | x86_64 | kevin | pass | pass | pass |
| boot-uname-aarch64 | aarch64 | dave | pass | pass | pass |
| kernel-early-boot-aarch64 | aarch64 | dave | pass | pass | pass |
| kernel-exc-aarch64 | aarch64 | dave | pass | pass | pass |
| kernel-paging-aarch64 | aarch64 | dave | pass | pass | pass |
| kernel-timer-aarch64 | aarch64 | dave | pass | pass | pass |

## Operational note

An earlier attempt at run 2 was interrupted by the harness after 19/27 cases (the minion
session was stopped, not a case failure). It is not one of the three runs above. The orphaned
QEMU process from that attempt was terminated before run 2 was restarted from the beginning.

## Scope

Test-only evidence: no kernel, gate-script or test-case edit was made. The generated
`docs/kernel-gate.md` was restored to its committed content so this change owns only this
document. Pi 4 remains UNVERIFIED and no tag is claimed.
