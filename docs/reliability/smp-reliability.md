<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# SMP reliability evidence (S85g, rework round 4)

- Date: 2026-11-18
- Product base: `eb3ac161` (origin/master merged into `jerry/S85g` as `45209b77`)
- Measured samples: `6807dc1f` (round 3, shared host) and `eb3ac161` (release-login, round 4)
- Host: i7-12700H, Linux 5.19.0-50-generic, QEMU 6.2.0 (Debian 1:6.2+dfsg-2ubuntu6.31),
  OVMF 2022.02-3ubuntu0.22.04.7, `/dev/kvm` + `kvm_intel`. The host is shared/saturated.
- Status: **BLOCKED** for the S85 matrix; the fixes are tracked in S113/S108/S110/S119.
- No new heavy stress or benchmark run was made in round 4 (host saturated).

## Stress matrix (current evidence, commit-labelled)

| Sub-ticket | Harness | Measured at | Result | Owner / blocker |
|---|---|---|---|---|
| S85a | `tests/smp/lock-time/**` | `6807dc1f` | **BLOCKED**: strict TCG fails at iteration 8; no-memory/timer-only variants pass | lance → **S113** (stuart, TLB IPI delivery) |
| S85b | `tests/smp/memory/**` | `6807dc1f` | **BLOCKED**: 85 forbidden TLB-timeout lines in 1 iteration; report 3/3 at 88/85/90 | mark → **S113** (stuart) |
| S85c | `tests/smp/ipc/**` | `6807dc1f` | **GREEN**: TCG 10/10; KVM 1/2/4 PASS; 12,800 calls, 25,600 rings, 12,000 traces, zero loss | tim (no fix pending) |
| S85d | `tests/libc/target/**` | report + `6807dc1f` | **BLOCKED**: `libc-threads` 20/20 TCG timeouts; stdio concurrency 3/3 fail (`fputs` EOF, errno 0); `posix-alarm` 6/6 pass | otto/phil → **S119** (phil, libc stdio) |
| S85e | `tests/posix/fs/posix-fs/**` | report + `6807dc1f` | **BLOCKED**: KVM 2/4 boots; VFS stress fails at iteration 0 (ENOMEM/EBADF/EIO, stuck mount); TCG/`smp_user=off` same | mel/tim → **S108** (tim, scheme IPC/VFS) |
| S85f | `scripts/smp-aarch64.sh` | report | **BLOCKED**: aarch64 `smp-tlb` 20/23 (3 TLB flakes); x86 `libc-threads` hang 2/2; aarch64 KVM unavailable | carl/dave → **S113**; disk-tools gate **S110** (bob) |

S85c's earlier KVM verdict is superseded; the current KVM 1/2/4 runs are green. The other five
lanes remain red and are mapped to the four named blocks below.

## KVM AP and boot measurements (commit-labelled)

`sh scripts/qemu-kvm.sh --timeout 120` at `6807dc1f`:

```
qemu-kvm: PASS -smp 1
qemu-kvm: PASS -smp 2
qemu-kvm: PASS -smp 4
qemu-kvm: PASSED -smp 1,2,4 iterations=1
```

`sh scripts/boot-time.sh --smp 4` at `6807dc1f` (shared host, 3 runs):

| run | firmware (ms) | kernel→login (ms) | total (ms) |
|---|---:|---:|---:|
| 1 | 4,135 | 382 | 4,518 |
| 2 | 6,796 | 636 | 7,433 |
| 3 | 7,489 | 694 | 8,183 |
| median | 6,796 | **636** | 7,433 |
| p90 | 7,489 | **694** | 8,183 |

OS gate **PASS** (kernel→login 636 ms < 1 s). S84's reference at `4ceb76b3` is 409 ms. The full
UEFI start→login is 4.5-8.2 s because OVMF dominates; the direct path is the S89 target.

TCG `--tcg --runs 1` at `6807dc1f` (loaded host): kernel→login 24,663 ms; total 65,244 ms
(report only).

## Direct boot (S89) — measured, load-sensitive

`sh scripts/boot-direct.sh --arch x86_64` at `6807dc1f` (shared host): exit 2, kernel median
571.7 ms / p90 873.9 ms; target <200 ms **MISSED**.

`sh scripts/boot-direct.sh --mode full` at `6807dc1f` (**loaded host**): exit 2, login median
2,300.9 ms / p90 2,720.8 ms; target <1 s **MISSED**. S89's clean sample is 755 ms / p90 923 ms
(MET), and its loaded service-asserting sample is 1,080 ms. The direct gate is marginal and
load-sensitive; the loaded sample is labelled and not presented as a clean pass.

## Login shell

- `cargo xtask qtest --case brush-login --arch x86_64`: **PASS** (brush identity, completion, no
  rsh; measured at `6807dc1f`, 226,403 ms under load).
- `cargo xtask qtest --case release-login --arch x86_64`: **PASS** at `eb3ac161` (36,952 ms).
  The earlier round-3 "external commands fail" verdict is **stale**; S96/S109 landed.

## Remaining reliability blocks (exact)

| ID | Owner | Scope / gate |
|---|---|---|
| **S113** | stuart | SMP TLB target IPI delivery; 50/50 TCG/KVM memory+lock stress and 50/50 KVM boot loops. Covers S85a/b/f. |
| **S108** | tim | Scheme IPC reply capture under concurrent VFS; `posix-fs-smp` + `libc-threads`. Covers S85e. |
| **S110** | bob | Dynamic scheme namespace and disk tools; `disk-tools` + UFS demo. The remaining disk-tools/UFS gate, not an S85 stress row. |
| **S119** | phil | libc stdio concurrency; `smp-libc-concurrency` TCG/KVM 1/2/4. Covers the S85d stdio failures. |

The umbrella closes only when those land green; none is waived.

## Cross-OS benchmarking

Per **DR-53**, ravenmark owns the Illumos/BSD/Linux baseline and comparison (images, versions,
URLs, checksums, identical QEMU/KVM harness). S85g keeps only the Raven-side numbers above and does
not run or publish a cross-OS comparison; no borrowed or invented numbers.

## Reproduction (evidence only; round 4 made no new heavy runs)

```sh
sh scripts/qemu-kvm.sh --timeout 120
sh scripts/boot-time.sh --smp 4
sh scripts/boot-time.sh --tcg --runs 1
sh scripts/boot-direct.sh --arch x86_64
sh scripts/boot-direct.sh --mode full
cargo xtask qtest --case brush-login --arch x86_64
cargo xtask qtest --case release-login --arch x86_64   # PASS on eb3ac161
sh tests/smp/ipc/run.sh kvm 4 on
sh tests/smp/lock-time/run.sh tcg
sh tests/smp/memory/run.sh tcg --iterations 1
```

No test was weakened and no failure was retried into a pass. The S85 umbrella remains BLOCKED on
S113/S108/S110/S119.
