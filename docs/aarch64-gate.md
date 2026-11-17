<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# aarch64 QEMU release-parity gate

Recorded regression sweep of the aarch64 QEMU surface at commit `86166d74`
(`dave/A8b`, from `origin/master`, which already contains the S76/S76b/S76c merges).

Command set:

```
cargo xtask qtest --case <case> --arch aarch64
```

`proc-death-cleanup` is declared `arch = "x86_64"`; it is included because the A8 sweep ran it on
aarch64 and the release gate needs that row. Its x86-64 control run is recorded below.

| Case | Result |
|---|---|
| `kernel-early-boot-aarch64` | pass |
| `kernel-exc-aarch64` | pass |
| `kernel-paging-aarch64` | pass |
| `kernel-timer-aarch64` | pass |
| `sched-kernel-threads-aarch64` | pass |
| `aarch64-libc` | pass |
| `boot-uname-aarch64` | pass |
| `scheme-aarch64` | pass |
| `vfs-aarch64` | pass |
| `pipe-fork` | pass |
| `proc-death-cleanup` | **fail** (aarch64 and x86-64) |
| `trace-context` | pass |
| `time-entropy` | pass |
| `smp-user-aarch64` | pass |
| `smp-user-off-aarch64` | pass |
| `smp-tlb-aarch64` | pass |
| `smp-soak-aarch64` | pass |
| `smp-soak-off-aarch64` | pass |
| `trace-demo` | pass |

No case timed out. Eighteen of nineteen rows pass. `vfs-aarch64`, red in the A8 sweep at
`51e6d38f`, now passes: the K54 UFS probe's relative `Readlink` succeeds, so the vfsd/UFS fix
landed between the two sweeps.

## Failing case

`proc-death-cleanup` fails on both architectures on this tree. This is a shared kernel regression,
not an aarch64-specific one.

aarch64 (`cargo xtask qtest --case proc-death-cleanup --arch aarch64`, exit 1; reproduced exactly
on a second run, same `ELR`/`FAR`/`ESR`):

```
s69: exit s69-owner processes=3 zombies=1 handles=0 lineage=13 timers=0 ... stack_fresh=10 stack_cached=0 heap=1071216
process: child exited (77)
s69: server sync drained=1 replied=0
exception: sync-cur-spx EC=0x25 (data abort same EL) ESR=0x0000000096000047 ELR=0xffffffff80234a2c FAR=0xffffc00000020000 SPSR=0x00000000000003c5
x00=0xffffc0000009c800 x01=0x0000000000000016 ... x29=0xffffc00000021120 x30=0xffffffff802c6eb0
exception: halted
```

* `ELR=0xffffffff80234a2c` is inside `__exception_vectors` (`+0x22c` from the table base
  `0xffffffff80234800`), so the fault happened while the vector prologue was saving an earlier
  exception's frame.
* `ESR=0x0000000096000047`: same-EL data abort, `WnR=1`, `DFSC=0x07` (write translation fault,
  level 3).
* `FAR=0xffffc00000020000` is exactly page-aligned, so the interrupted `SP` was one trap-frame
  (288 bytes) above an unmapped page bottom: the classic signature of a kernel stack exhausted to
  its guard page.
* The failing expectations are `^s69: cycle 25$` onward (`cycle 50`, `cycle 75`,
  `cycles done=100`, `client eio ok`, `server death client ok`, `server ready`, `server
  reregistered`, the `exit s69-drain ...` counter line, `cleanup ok`, `init exited (0)`); the
  forbidden `exception:` lines are what marks the case red.

x86-64 control (`cargo xtask qtest --case proc-death-cleanup --arch x86_64`, exit 1):

```
EXCEPTION: page fault (vector 14, error 0x0)
EXCEPTION: double fault (vector 8, error 0x0)
```

Same point in the run (`s69` teardown, `stack_fresh=10 stack_cached=0`), so the defect is in the
shared process-teardown / kernel-stack path, not in `kernel/src/arch/aarch64/**`.

### Window and candidates

The A8 sweep recorded this case as passing at `51e6d38f` (2026-11-17T05:40+01:00). The current
tree adds, in `kernel/src/sched`, `kernel/src/process` and `kernel/src/memory`:

* `fba21fb7` A5c — wire `raven-frames` buddy and per-CPU caches into the kernel (frame allocator);
* `8bada7bf` A5c — base the buddy at the lowest usable frame;
* `f6205a1d`/`0bffc0df` S75 — run user threads on every x86-64 CPU;
* `c8d46f91`/`75220546`/`c94fd28c`/`3f6218df` S76/S76b — aarch64 per-CPU SMP, FP/SIMD, shootdown.

The symptom is a stack/guard-page fault in the teardown path on both architectures, so the
frame-allocator and kernel-stack-reuse changes (A5c) and the shared scheduler changes (S75) are the
prime suspects. This ticket is test-only and did not bisect further; the exact case, command and
register dump above are the handoff.

**Owner:** shared kernel — memory/frames (mark) and scheduler (stuart); not the aarch64 arch lane.
A separate ticket should fix it; the aarch64 parity row stays red until then.

## Outcome

Eighteen of nineteen aarch64 rows pass, including every new SMP/TLB/soak case and `vfs-aarch64`.
One shared regression (`proc-death-cleanup`) is red on both architectures and is handed to its
owners with the exact output. This is QEMU parity only; there is no Pi 4 board claim (Pi 4 stays
UNVERIFIED).
