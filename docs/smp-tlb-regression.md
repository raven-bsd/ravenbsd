<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# S159 TCG4 SMP TLB/IPI regression record

Status: **TLB/IPI path clean; TCG4 posix-fs-smp acceptance load-blocked.** Reconstructed on
`origin/master` `604a93671a1dcc96e2be5b1df5b7018f0d98d8d6`. The deterministic S85b TCG TLB
stress passes 6/6 with zero `smp: tlb shootdown timeout` lines. The TCG4 `posix-fs-smp` case
times out its VFS workers under the 15-lane host load, with no TLB timeout line; KVM4 passes.
No kernel change is justified by this evidence.

## Reproduction

Commands (each foreground, proportional cap):

```sh
timeout 420 cargo xtask qtest --case posix-fs-smp --arch x86_64
timeout 420 cargo xtask qtest --case posix-fs-smp-kvm4 --arch x86_64
timeout 240 sh tests/smp/memory/run.sh tcg --iterations 1
timeout 240 sh tests/smp/memory/run.sh tcg --iterations 5 --no-build
```

| Run | Result | Duration | TLB line | Pending workers / phase |
|---|---|---:|---|---|
| posix-fs-smp TCG4 #1 | fail | 220,115 ms | none | `0:16 1:16 2:16 3:16 4:30 5:38 6:38 7:30` |
| posix-fs-smp TCG4 #2 | fail | 222,786 ms | none | `0:22 1:20 2:22 3:22 4:32 5:32 6:32 7:32` |
| smp-memory TCG 1 | pass | 14,270 ms | 0 | `tlb_stale=0` |
| smp-memory TCG 5 | pass | worst 24,561 ms, p90 24,561 ms, avg 18,258 ms | 0 | `tlb_stale=0` |
| posix-fs-smp KVM4 | pass | - | - | - |

The TCG4 serials are `build/qtest/posix-fs-smp.log` and the qtest JSON is
`build/qtest/posix-fs-smp.json`. The S85b logs are under `build/smp-memory/tcg/run-*/`.
The TCG4 failure is the stress fixture's own 220 s worker deadline; the guest logs show large
latency samples (mount p50 about 13 s, p90 about 17 s) while the host load was 57-69. The same
case is green under KVM4.

## TLB/IPI analysis

S113's target-delivery fix is present: `kernel/src/smp/shootdown.rs` uses the interruptible
`ActiveGuard` instead of the old global IRQ-masked lock, and the shooter accepts either
`ACKED[cpu] == sequence` or `LAST[cpu] == sequence`. The S85b TLB stress exercises the same
mailbox with concurrent map/unmap faults and completes 6/6 runs with `tlb_stale=0` and no timeout
line. The S108 state line `seq=94 last=94 ... state=2 syscall=0 nr=0x68 user_if=1 irq_age=125`
shows the target eventually handled the IPI after the 100 ms deadline; with no such line in the
current TCG4 runs, the TCG4 failure is not the S113 TLB regression.

The TCG4 `posix-fs-smp` failure is a VFS/mount worker stall under host contention. It must be
rerun on a quiet host before assigning it to a kernel file; the latency delta (quiet baseline
mount p50 62 us in S85e versus 12.9 s here) is dominated by host CPU starvation. No timeout was
raised and no retry was used to hide the failure.

## Donor reconciliation

The TLB/IPI path was ported under S113. No code is copied in this evidence ticket.

| Topic | Illumos (behaviour only) | BSD file/licence | Linux (read-only) | Decision |
|---|---|---|---|---|
| TLB/IPI target delivery | `usr/src/uts/i86pc/os/x_call.c`, `usr/src/uts/i86pc/vm/hat_i86.c` design; no code copied | NetBSD `sys/arch/x86/x86/x86_tlb.c`, `sys/arch/x86/x86/ipi.c`, `sys/kern/subr_xcall.c` (BSD-2); no copy | `flush_tlb_mm_range` / `smp_call_function_many` outcomes | First-party S113 mailbox/ActiveGuard is already merged; no new port |
| Syscall/wait interruptibility | `post_syscall`/CPU state design | NetBSD/FreeBSD syscall-return behaviour | `syscall_exit_to_user_mode` outcomes | Existing S113 IF policy; no new code in S159 |

Revisions: FreeBSD `133344405f532bd54ff3629a77d4c4e062f01fdb`, NetBSD
`36eddebcc36cb0775aa2962005b9c05419a6ff01`, Illumos
`586678ac1bcbd77644a6ec4dac7f79d676cfb4ec` (read-only). No CDDL/GPL code was copied.

## Open gaps

1. **TCG4 acceptance (OBSERVED, load-blocked):** `posix-fs-smp` fails its 220 s worker deadline
   under host load 57-69 with no TLB line; KVM4 passes. A quiet-host rerun is required before a
   kernel fix is assigned.
2. **S108 blocker:** the TLB/IPI timeout path is closed by the 6/6 S85b clean TCG runs; the
   mount-worker stall remains a VFS/load issue for the S85e owner or a quiet-host rerun.
3. **Host load:** the 15-lane load makes the 4-vCPU TCG guest's 50-iteration VFS stress exceed its
   fixed worker deadline. No cap or test deadline was raised.
