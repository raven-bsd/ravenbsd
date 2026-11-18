<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# RavenBSD reliability evidence — S85g (rework round 4)

- Date: 2026-11-18
- Product base: `eb3ac161` (origin/master merged into `jerry/S85g` as `45209b77`)
- Measured samples: `6807dc1f` (round 3, shared host) and `eb3ac161` (release-login, round 4)
- Status: **DONE for this ticket's evidence/docs.** The S85 umbrella remains **BLOCKED** on the
  named S113/S108/S110/S119 work. No release reliability claim.
- Rule: measured Raven numbers only, each labelled with its commit and host state. No cross-OS
  benchmark: **ravenmark owns Illumos/BSD/Linux (DR-53)**, never duplicated here.

## Current state (all observed)

| Item | State |
|---|---|
| S83 KVM AP bring-up | **DONE/merged**; KVM `-smp 1/2/4` PASS with login |
| S84 boot-time | **DONE/merged**; 409 ms kernel→login at `4ceb76b3`; fresh 636 ms at `6807dc1f` |
| S89 direct boot | **DONE/merged**; clean full login 755 ms / p90 923 ms; loaded 1,080 ms |
| S95 network | **DONE/merged**; netd/resolvd up, eth0 10.0.2.15/24 |
| S96 brush/no-rsh | **DONE/merged**; `brush-login` PASS; **`release-login` PASS** (36,952 ms at `eb3ac161`) |
| `tests/smp/**` | **present**: `aarch64`, `boot`, `ipc`, `lock-time`, `memory`, `x86_64` |
| S85a-f | S85c **GREEN**; S85a/b/d/e/f still **BLOCKED** (see below) |

The previous "S83/S84 not landed", "tests/smp does not exist" and "release-login external commands
fail" statements are **stale and corrected**; `release-login` passes on `eb3ac161`.

## Fresh measured evidence (commit and host labelled)

| Command / sample | Commit | Host state | Result |
|---|---|---|---|
| `sh scripts/qemu-kvm.sh --timeout 120` | `6807dc1f` | shared host | **PASS** `-smp 1/2/4` with login |
| `sh scripts/boot-time.sh --smp 4` | `6807dc1f` | shared host | **PASS**: kernel→login 382/636/694 ms (median 636 ms, p90 694 ms); firmware 4,135/6,796/7,489 ms; total 4,518/7,433/8,183 ms |
| `sh scripts/boot-time.sh --tcg --runs 1` | `6807dc1f` | shared/loaded | kernel→login 24,663 ms; total 65,244 ms (TCG, report only) |
| `sh scripts/boot-direct.sh --arch x86_64` | `6807dc1f` | shared host | **exit 2**: kernel median 571.7 ms / p90 873.9 ms; target <200 ms MISSED |
| `sh scripts/boot-direct.sh --mode full` | `6807dc1f` | **loaded host** | **exit 2**: login median 2,300.9 ms / p90 2,720.8 ms; target <1 s MISSED (S89 clean sample 755/923 ms; loaded 1,080 ms) |
| `cargo xtask qtest --case release-login --arch x86_64` | `eb3ac161` | shared host | **PASS**, 36,952 ms |
| `sh tests/smp/ipc/run.sh kvm 4 on` / `kvm 2 on` | `6807dc1f` | shared host | **PASS** (S85c; 12,800 calls, 25,600 rings, 12,000 traces, zero loss; KVM4 p90 315.6 ms/worst 395.3 ms; KVM2 p90 1.091 s/worst 1.342 s) |
| `sh tests/smp/lock-time/run.sh tcg` | `6807dc1f` | shared host | **FAIL** at iteration 8 (S85a strict 50-iteration gate) |
| `sh tests/smp/memory/run.sh tcg --iterations 1` | `6807dc1f` | shared host | **FAIL**: 85 forbidden TLB-timeout lines (S85b) |

No new benchmark or heavy stress run was made in round 4 (host saturated). The numbers above are
kept as measured, with their commit and host state; the loaded-host direct-full sample is labelled
as such and is not presented as a clean gate.

## Stress matrix and the exact remaining blocks

| Lane | State | Remaining blocker / owner |
|---|---|---|
| S85a locks/time/IPIs | BLOCKED | TLB target IPI delivery → **S113** (stuart) |
| S85b memory/paging/TLB | BLOCKED | TLB target IPI / IRQ-masked syscall → **S113** (stuart) |
| S85c IPC/rings/tracing | **GREEN** | TCG 10/10 + KVM 1/2/4 PASS; no fix pending |
| S85d POSIX/pthread/libc | BLOCKED | stdio concurrency (`fputs` EOF, errno 0), `libc-threads` hangs → **S119** (phil) |
| S85e VFS/filesystem | BLOCKED | scheme IPC reply capture under concurrent VFS → **S108** (tim) |
| S85f aarch64/TCG | BLOCKED | aarch64 TLB flake + KVM unavailable → **S113** (stuart) / **S110** (bob, disk-tools/UFS gate) |

The four named remaining reliability blocks are:

1. **S113** (stuart): SMP TLB target IPI delivery; gate 50/50 TCG/KVM memory and lock stress plus
   50/50 KVM boot loops. Covers S85a/S85b/S85f.
2. **S108** (tim): scheme IPC reply capture under concurrent VFS; gate `posix-fs-smp` and
   `libc-threads`. Covers S85e.
3. **S110** (bob): dynamic scheme namespace and disk tools; gate `disk-tools` and UFS demo. This is
   the remaining disk-tools/UFS gate, not an S85 stress row.
4. **S119** (phil): libc stdio concurrency; gate `smp-libc-concurrency` TCG/KVM 1/2/4. Covers the
   S85d stdio failures.

## Cross-OS benchmarking

**Benchmarking is not this ticket's deliverable.** Per **DR-53**, ravenmark owns the
Illumos/BSD/Linux baseline and comparison, including the image set, versions, URLs, checksums and
the identical QEMU/KVM harness. S85g keeps the Raven-side numbers above and does not run or publish
a cross-OS comparison; no number is borrowed or invented.

## DR-52 donor reconciliation

| Topic | Illumos finding | BSD file/licence | Linux finding | Decision |
|---|---|---|---|---|
| Boot-time measurement | SMF boot phases/milestones; design-only | FreeBSD `usr.sbin/bhyve` (BSD-2) is a VMM, not a timing harness; no permissive comparator | systemd boot-phase timing/`perf`; read-only | First-party `scripts/boot-time.sh`; no donor code |
| Direct PVH loader | boot stages/loader handoff; design-only | FreeBSD loader/`bhyve` (BSD-2) as a reference; no copy | PVH `hvm_start_info`; read-only | S89 first-party `boot/direct` loader; no donor code |
| SMP AP bring-up | `mp_startup`, `xcalls`, TLB ack; design-only | FreeBSD `mp_machdep.c`/NetBSD `cpu.c` (per-file BSD; reference only) | `smpboot.c`/`tlb.c`; read-only | S83 clean-room fix; no donor code |
| Stress campaigns | xcalls/turnstiles/TLB ack; design-only | NetBSD ATF (BSD-2) is the existing harness donor; S85 fixtures are first-party | scheduler/IPC/TLB outcomes; read-only | First-party `tests/smp/**`; exact failures in the S85 reports |
| Boot-phase table | boot milestones; no code | FreeBSD loader/rc phases (BSD-2) as naming reference | systemd-analyze phase split; read-only | Firmware/kernel→login/total separately; no borrowed numbers |
| Cross-OS baseline | ravenmark's comparison, not this ticket | ravenmark owns the BSD image set | ravenmark owns the Linux image set | **DR-53:** ravenmark owns the benchmark; S85g does not duplicate it |

No donor code was copied, translated or closely paraphrased.

## Reproduction (evidence only; no new benchmark/stress runs in round 4)

```sh
cargo xtask qtest --case release-login --arch x86_64   # PASS on eb3ac161
sh scripts/qemu-kvm.sh --timeout 120                   # measured at 6807dc1f
sh scripts/boot-time.sh --smp 4                        # measured at 6807dc1f
sh scripts/boot-direct.sh --mode full                  # loaded-host sample at 6807dc1f
sh tests/smp/ipc/run.sh kvm 4 on                       # S85c green
```

Verdict: **DONE** for S85g's evidence/docs (current commit, stale claims corrected, cross-OS handed
to ravenmark, remaining blocks named). The **S85 umbrella remains BLOCKED** on
S113/S108/S110/S119; DR-49 is not consistently met on the loaded host. No tag/release claim.
