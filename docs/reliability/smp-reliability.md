<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# SMP reliability evidence (S85g, rework round 3)

- Date: 2026-11-18
- Product base: `6807dc1f` (origin/master `b6163e85` merged into `jerry/S85g`)
- Host: i7-12700H, Linux 5.19.0-50-generic, QEMU 6.2.0 (Debian 1:6.2+dfsg-2ubuntu6.31),
  OVMF 2022.02-3ubuntu0.22.04.7, `/dev/kvm` + `kvm_intel`
- Image: `build/boot-time/release.img`, 320 MiB, SHA-256
  `514c7be8c205c8291c58363326f7a9db2b2a104198c5b2634420a48529b9e9d8`, banner `6807dc1f`
- Status: **BLOCKED** for the S85 matrix. The harnesses exist; five lanes still have real failures.

## Stress matrix (current master + owning-lane reports)

| Sub-ticket | Harness | Current evidence | Result | Owner / file |
|---|---|---|---|---|
| S85a | `tests/smp/lock-time/**` | fresh `run.sh tcg`: fail at **iteration 8**; report's no-memory and timer-only variants pass 50/50 | **BLOCKED** | lance; stuart / `kernel/src/smp/shootdown.rs:122` |
| S85b | `tests/smp/memory/**` | fresh 1-iteration run: **85 forbidden TLB-timeout lines**; report 3/3 clean runs at 88/85/90 | **BLOCKED** | mark; stuart / `kernel/src/smp/shootdown.rs:81` |
| S85c | `tests/smp/ipc/**` | TCG 10/10 (p90 44 ms-2.29 s, worst 0.19-3.79 s); fresh **KVM 1/2/4 PASS** on this tree (KVM4 p90 315.6 ms/worst 395.3 ms; KVM2 p90 1.091 s/worst 1.342 s); 12,800 calls, 25,600 rings, 12,000 traces, zero loss | **GREEN (measured)** | tim |
| S85d | `tests/libc/target/**` | `libc-threads` 20/20 TCG timeouts (60.8-66.5 s); malloc/stdio concurrency 3/3 fail; `posix-alarm` 6/6 pass | **BLOCKED** | otto; libc-thread/stdio |
| S85e | `tests/posix/fs/posix-fs/**` | KVM 2/4 boots but the VFS stress fails at **iteration 0** (ENOMEM/EBADF/EIO, stuck mount worker); TCG same; `smp_user=off` same | **BLOCKED** | mel; stuart/tim (vfsd/ramfsd/router) |
| S85f | `scripts/smp-aarch64.sh` | aarch64 `smp-tlb` 20/23 (3 TLB flake, `kernel/src/sched/smp_shim.rs:225`); x86 `libc-threads` hang 2/2; aarch64 KVM unavailable | **BLOCKED** | carl; dave/C57, S83 |

S85c's earlier BLOCKED verdict was the pre-S83 KVM AP stop; the current KVM 2/4 runs supersede it.
S85a/b still fail on the current tree with S83-r2 merged, so the TLB lost-ack is not fully fixed.
S85e's KVM reset is gone; its VFS/IPC failures are independent of the KVM AP path.

## KVM AP and boot measurements (current tree)

`sh scripts/qemu-kvm.sh --timeout 120`:

```
qemu-kvm: PASS -smp 1
qemu-kvm: PASS -smp 2
qemu-kvm: PASS -smp 4
qemu-kvm: PASSED -smp 1,2,4 iterations=1
```

`sh scripts/boot-time.sh --smp 4` (KVM 4 CPU, 3 runs):

| run | firmware (ms) | kernel→login (ms) | total (ms) |
|---|---:|---:|---:|
| 1 | 4,135 | 382 | 4,518 |
| 2 | 6,796 | 636 | 7,433 |
| 3 | 7,489 | 694 | 8,183 |
| median | 6,796 | **636** | 7,433 |
| p90 | 7,489 | **694** | 8,183 |

Gate: **PASS** (kernel→login median 636 ms < 1 s). The S84 reference at `4ceb76b3` is 409 ms.
The full QEMU start→login is >1 s because OVMF dominates; that is the S89 direct-path target, not
this OS gate.

`sh scripts/boot-time.sh --tcg --runs 1`: kernel→login 24,663 ms, total 65,244 ms (TCG, report
only; host loaded).

## Direct boot (S89) on current tree

`sh scripts/boot-direct.sh --arch x86_64` (microvm, KVM, 3 runs): exit 2, kernel-banner median
571.7 ms / p90 873.9 ms; target <200 ms **MISSED** (host loaded; S89 clean sample was 348 ms).

`sh scripts/boot-direct.sh --mode full` (q35, KVM 4, 3 runs): exit 2, login median 2,300.9 ms /
p90 2,720.8 ms; target <1 s **MISSED** on this loaded host. S89's clean sample was 755 ms median /
923 ms p90 (MET), and its loaded service-asserting sample was 1,080 ms. The direct full gate is
therefore **marginal/load-sensitive**, not a clean release pass. The loader/network/services start;
the remaining cost is kernel early init and the S84/S95/S96 service work.

## Login shell

`cargo xtask qtest --case brush-login --arch x86_64`: **PASS** (brush identity, completion, no
rsh; 226,403 ms under load). `release-login`: **FAIL** after `raven# ls /bin` — no external-command
output within 30 s (`cat`/`ls`/`uname`/ping expectations unmatched; 76,653 ms). The brush shell is
the login shell and the network services are up; the external-command console path remains the
S96/S109 blocker. No usable-login PASS is claimed.

## Reproduction

```sh
sh scripts/qemu-kvm.sh --timeout 120
sh scripts/boot-time.sh --smp 4
sh scripts/boot-time.sh --tcg --runs 1
sh scripts/boot-direct.sh --arch x86_64
sh scripts/boot-direct.sh --mode full
cargo xtask qtest --case brush-login --arch x86_64
cargo xtask qtest --case release-login --arch x86_64
sh tests/smp/ipc/run.sh kvm 4 on
sh tests/smp/ipc/run.sh kvm 2 on
sh tests/smp/lock-time/run.sh tcg
sh tests/smp/memory/run.sh tcg --iterations 1
```

No test was weakened and no failure was retried into a pass. The umbrella remains BLOCKED until
the TLB/pthread/VFS/S96 owners land fixes and every campaign passes 50/50 or the documented
time-box.
