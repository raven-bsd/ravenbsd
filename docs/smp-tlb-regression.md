<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# TCG4 SMP TLB/IPI regression closure (S159)

Status: **NO-GO under the original failing load** (rework rev 177). The quiet-host
runs pass, but a 20-iteration TCG4 stress at load 88-126 failed **all 20**
iterations and produced two real `smp: tlb shootdown timeout` lines (iterations
4 and 10). The release TLB gate stays open until the stress is clean under load
or a kernel fix is traced.

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


## Rework round 2 (rev 177) — 20 iterations under load

Controlled load: 96 `yes` CPU burners on the 20-CPU host, in two waves so the
whole 20-iteration sweep runs under the original failing conditions.

| Wave | Iterations | Burners | Load 1/5/15 min at start | Load at stop |
|---|---|---|---:|---:|
| 1 | 1-5 | 96 | `28.08 / 16.76 / 24.10` | `102.29 / 103.96 / 88.70` |
| 2 | 6-20 | 96 | `64.65 / 76.04 / 79.81` | `103.05 / 102.23 / 103.13` (peak 126.45) |

Each iteration is
`timeout 420 cargo xtask qtest --case posix-fs-smp --arch x86_64 --no-build`
against the existing image. **All 20 iterations fail** (exit 1); no cap, case
timeout or worker deadline was changed.

| Iter | Result | Duration | Serial key |
|---:|---|---:|---|
| 1 | fail | 215,038 ms | mount p50 4.35 s; workers `0..3:16`, `4..7:36-37` |
| 2 | timeout | 303,844 ms | case timeout; no latency/race/done lines |
| 3 | fail | 209,095 ms | mount p50 4.27 s; workers `0..3:40`, `4..7:35-36` |
| 4 | fail | 211,036 ms | **TLB timeout**, see below |
| 5 | timeout | 301,636 ms | case timeout; no latency/race/done lines |
| 6 | fail | 208,658 ms | workers `0..3:13-14`, `4..7:31` |
| 7 | timeout | 301,312 ms | case timeout |
| 8 | fail | 303,098 ms | workers `0..3:13-14` |
| 9 | fail | 210,319 ms | workers `0..3:13-14`, `4..7:30-31` |
| 10 | fail | 209,826 ms | **TLB timeout**, see below |
| 11 | fail | 208,655 ms | workers `0..3:22-24`, `4..7:32` |
| 12 | fail | 209,584 ms | workers `0..3:16-24`, `4..7:35-36` |
| 13 | fail | 208,725 ms | workers `0..3:15-22`, `4..7:37` |
| 14 | fail | 212,441 ms | workers `0..3:14`, `4..7:34-35` |
| 15 | fail | 210,613 ms | workers `0..3:13-14`, `4..7:33` |
| 16 | fail | 210,166 ms | workers `0..3:13-14`, `4..7:30-38` |
| 17 | fail | 288,766 ms | workers `0..3:14-15`, `4..7:30-38` |
| 18 | fail | 220,442 ms | workers `0..3:14`, `4..7:32-33` |
| 19 | fail | 253,780 ms | workers `0..3:13-14`, `4..7:36` |
| 20 | fail | 207,400 ms | workers `0..3:13-14`, `4..7:30-37` |

### TLB timeouts

Two of the 20 iterations produced a real `tlb shootdown timeout`:

```text
# iteration 4  (/tmp/opencode/s159r2-tcg-4.serial)
smp: tlb shootdown timeout on cpu 3 seq=14 last=13 flushes=11 state=0 \
  syscall=1 pf=0 pfp=0 wq=0 nr=0x66 user_if=1 irq_age_ms=123 state_age_ms=123

# iteration 10 (/tmp/opencode/s159r2-tcg-10.serial)
smp: tlb shootdown timeout on cpu 0 seq=16994 last=16993 flushes=16925 state=1 \
  syscall=1 pf=0 pfp=0 wq=0 nr=0x1 user_if=1 irq_age_ms=103 state_age_ms=119
```

`state` is `0 = kernel`, `1 = user`, `2 = idle`; `nr=0x66` is
`Syscall::FSTAT` and `nr=0x1` is `Syscall::EXIT`. The two timeouts are therefore
different capture points: one target in the kernel handling `FSTAT`, one in user
mode after an `EXIT` syscall. Neither is an idle/stale mailbox: `last` is one
behind the current sequence and `flushes` is non-zero.

Both report `irq_age_ms` just over the 100 ms deadline (103 and 123 ms). The
timer is a 1000 Hz periodic tick and `note_irq` runs at the top of `sched::tick`,
so a CPU that is executing with interrupts enabled takes a tick every 1 ms.
An `irq_age` of 103-123 ms means that CPU missed 100+ consecutive ticks. That is
consistent with either (a) the host not scheduling the TCG vCPU for the whole
window, or (b) a guest section that keeps `IF=0` for the whole window. The
report does not yet distinguish them; the unmodified S113 fixture cannot tell
them apart either (it is green under the same load, below).

### S113 TLB repro under the same load

```text
timeout 300 sh tests/smp/memory/run.sh tcg --iterations 10 --no-build
iterations=10 planned=10; worst=11,372 ms; p90=11,063; average=10,288
last_sample=smp-memory: ... tlb_rounds=64 tlb_stale=0
no tlb shootdown timeout, exit 0
```

The dedicated mailbox/ActiveGuard fixture is 10/10 green at load 88-105, so the
standalone shootdown path is not sufficient to reproduce this. The heavier
`posix-fs-smp` workload (8 workers, mount/FSTAT/latency phases) is required.

### Verdict

The release TLB gate is **NO-GO**. A known SMP unreliability at the original
failing load is a blocker, not a flake waiver (rule 18). No product code, test,
case profile or deadline was changed in this round. Handoff: S83/Stuart owns
`kernel/src/smp/**`; the next step is to instrument the target CPU's `IF`/
interrupt-mask depth (or capture a host-side vCPU run-delay trace) so the
starvation and guest-`cli` hypotheses can be separated, then fix test-first under
this load.

Logs: `/tmp/opencode/s159r2-tcg-{1..20}.{log,json,serial}`,
`/tmp/opencode/s159r2-tlb10.log`.

