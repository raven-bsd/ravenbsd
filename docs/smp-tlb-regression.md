<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# TCG4 SMP TLB/IPI regression closure (S159)

Status: **closed as a load-sensitive flake, not a TLB/IPI defect** on
`origin/master` `530663b2`.

## Evidence

Bounded acceptance on the current tree:

| Run | Command | Result | Duration | Serial markers |
|---|---|---|---:|---|
| TCG4 #1 | `timeout 420 cargo xtask qtest --case posix-fs-smp --arch x86_64` | pass | — | `smp: 4 cpus online`, `smp-fs: done workers=8 iterations=50`, `init exited (0)` |
| TCG4 #2 | same | pass | — | same |
| TCG4 #3 | same | pass | 182,512 ms | same; `latency mount_us p50=1806891 p90=3112345 max=9625406` |
| KVM4 | `timeout 420 cargo xtask qtest --case posix-fs-smp-kvm4 --arch x86_64` | pass | 56,193 ms | `smp-fs: race successes=50/50`, `done workers=8 iterations=50` |
| TLB stress | `timeout 600 sh tests/smp/memory/run.sh tcg --iterations 3` | pass 3/3 | worst 8,328 ms | `tlb_rounds=64 tlb_stale=0`, no `smp: tlb shootdown timeout` |

No `smp: tlb shootdown timeout` line appears in any passing TCG4 serial. The
S113 TLB mailbox/`ActiveGuard`/ACK path is exercised by the S85b stress with
`tlb_stale=0` on three TCG iterations.

## Root cause

The earlier failures on `604a9367` (dave/S159 BLOCKED, otto/S159 PARTIAL) were
host-scheduling contention, not a lost shootdown IPI:

- the failing serials showed the target CPU idle (`state=2`) with a stale
  `nr=0x68` from an earlier scheme `RECV`, `irq_age_ms=125`, and no TLB timeout
  line in the later runs;
- the 180 s fixture worker deadline expired while mount latency was p50 12.9 s
  under a host load of 57–89;
- the same image passes on KVM4, and the deterministic S113 TLB reproducer is
  green.

On this run (host load ~24) TCG4 passes 3/3 and the TLB stress is clean. The
mount latency is still elevated (`p50=1.8 s` on TCG vs `675 ms` KVM4), which is
the remaining host-contention sensitivity; no timeout, cap, or worker deadline
was raised.

## S108 blocker reference

The S108 TLB/IPI blocker is closed: TCG4 `posix-fs-smp` is green on every
bounded iteration, and the S113 TLB stress has zero stale/timeout rounds. The
VFS/mount phase is recorded as load-sensitive but not a kernel TLB defect.

## Donor reconciliation

No product code was changed. The TLB/IPI path was ported under S113; the donor
set is repeated for the record.

| Topic | Illumos (behaviour only) | BSD (permissive reference) | Linux (outcome only) | Decision |
|---|---|---|---|---|
| TLB shootdown target/ACK | `usr/src/uts/i86pc/os/x_call.c`, `hat_i86.c` design; no copy | NetBSD `sys/arch/x86/x86/x86_tlb.c`, `sys/arch/x86/x86/ipi.c`, `sys/kern/subr_xcall.c` (BSD-2) | `flush_tlb_mm_range` / `smp_call_function_many` outcome | S113 first-party mailbox/ActiveGuard is healthy; no patch |
| scheme `RECV` wait | `post_syscall`/CPU-state design | NetBSD/FreeBSD syscall-return behaviour | `syscall_exit_to_user_mode` outcome | existing IF policy; no new code |
| TCG4 VFS/mount | VFS/mount design; no copy | NetBSD/FreeBSD VFS/mount behaviour | read-only | load-sensitive; quiet-host runs pass |

No CDDL/GPL code copied. FreeBSD reference revision
`133344405f532bd54ff3629a77d4c4e062f01fdb`, NetBSD
`36eddebcc36cb0775aa2962005b9c05419a6ff01`, Illumos
`586678ac1bcbd77644a6ec4dac7f79d676cfb4ec`.

## Gates

- `posix-fs-smp` TCG4: 3/3 pass; KVM4 pass.
- S113 TLB stress: 3/3 pass, `tlb_stale=0`.
- `make check-fast`: **PASSED** (420 s cap; all steps ok).

Logs: `/tmp/opencode/s159-tcg{1,2,3}.log`, `/tmp/opencode/s159-kvm4.log`,
`/tmp/opencode/s159-tlb2.log`, `/tmp/opencode/s159-checkfast.log`.
