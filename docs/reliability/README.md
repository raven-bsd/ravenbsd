<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# RavenBSD reliability evidence — S85g (rework round 3)

- Date: 2026-11-18
- Product base: `6807dc1f` (origin/master `b6163e85` merged into `jerry/S85g`)
- Status: **PARTIAL / umbrella BLOCKED.** The Raven-side evidence is refreshed on current
  master; the S85 matrix still has real failures and DR-49 is not consistently met. No release
  reliability claim.
- Rule: measured numbers only. A missing or flaky matrix is **BLOCKED**, never a pass.

## Evidence matrix (current master)

| Ticket | Harness | State on `6807dc1f` | Current measured result | Owner |
|---|---|---|---|---|
| S85a locks/time/IPIs | `tests/smp/lock-time/**` | **BLOCKED** | strict TCG 50-iteration run fails at **iteration 8** (`smp: tlb shootdown timeout` / no progress); no-memory and timer-only variants pass, so TLB pressure is required | lance; stuart/S83 |
| S85b memory/paging/TLB | `tests/smp/memory/**` | **BLOCKED** | 1 iteration: **85 forbidden TLB-timeout lines**; report's 3 clean runs were 88/85/90; root cause `kernel/src/smp/shootdown.rs:81` (`IrqSpinLock` while waiting for ACKs) | mark; stuart/S83 |
| S85c IPC/rings/tracing | `tests/smp/ipc/**` | **GREEN for the measured matrix** | TCG 10/10 pass (p90 44 ms-2.29 s, worst 0.19-3.79 s); **KVM 1/2/4 PASS on this tree** (KVM4 p90 315.6 ms/worst 395.3 ms; KVM2 p90 1.091 s/worst 1.342 s), 12,800 calls, 25,600 rings, 12,000 traces, zero loss | tim |
| S85d POSIX/pthread/libc | `tests/libc/target/**` | **BLOCKED** | `libc-threads` 20/20 TCG timeouts; malloc/stdio concurrency 3/3 fail; `posix-alarm` 6/6 pass | otto; libc-thread/stdio |
| S85e VFS/filesystem | `tests/posix/fs/posix-fs/**` | **BLOCKED** | KVM 2/4 now boots but the VFS stress fails at **iteration 0** (ENOMEM/EBADF/EIO, stuck mount worker); TCG same; the failure also occurs with `smp_user=off`, so it is not TLB-only | mel; stuart/tim (vfsd/ramfsd/router) |
| S85f aarch64/TCG | `scripts/smp-aarch64.sh` | **BLOCKED** | aarch64 `smp-tlb` 20/23 (3 TLB flakes, `kernel/src/sched/smp_shim.rs:225`); x86 `libc-threads` hang 2/2; aarch64 KVM unavailable in QEMU 6.2 | carl; dave/C57, S83 |
| S83 KVM AP bring-up | `sh scripts/qemu-kvm.sh` | **DONE** | KVM `-smp 1/2/4` PASS with login (fresh run on `6807dc1f`) | stuart |
| S84 boot-time | `sh scripts/boot-time.sh --smp 4` | **DONE, load-sensitive** | fresh sample: kernel→login median **636 ms**, p90 694 ms (PASS), firmware median 6,796 ms, total 7,433 ms; S84 reference at `4ceb76b3`: 409 ms | dave |
| S89 direct boot | `sh scripts/boot-direct.sh` | **DONE, marginal** | `--mode full`: clean S89 sample login median 755 ms / p90 923 ms (MET), loaded sample 1,080 ms; current loaded rerun **2,301 ms / p90 2,721 ms (MISSED)**; probe kernel 571.7 ms (target <200 ms MISSED) | kevin/stuart |
| S95 network | `scripts/qemu-net.sh`; resolvd | **DONE** | netd/resolvd up, eth0 10.0.2.15/24, gateway 10.0.2.2 in the release transcript | bob |
| S96 brush/no-rsh | `tests/qtest/cases/brush-login.toml` | **WORKING** | `brush-login` **PASS** (brush identity, completion, no rsh); `release-login` still **FAILS** after `raven# ls /bin` (no serial output within 30 s); S96 blocker | phil |

S85c is green; the other five campaigns still have real failures/flakes. The umbrella stays open.

## Fresh S85g measurements (OBSERVED on `6807dc1f`)

| Command | Result |
|---|---|
| `sh scripts/qemu-kvm.sh --timeout 120` | **PASS** `-smp 1`, `2`, `4` with login |
| `sh scripts/boot-time.sh --smp 4` | **PASS**: kernel→login 382/636/694 ms (median 636 ms, p90 694 ms); firmware 4,135/6,796/7,489 ms; total 4,518/7,433/8,183 ms |
| `sh scripts/boot-time.sh --tcg --runs 1` | kernel→login 24,663 ms; total 65,244 ms (TCG, report only; host loaded) |
| `sh scripts/boot-direct.sh --arch x86_64` | **exit 2**: kernel banner median 571.7 ms / p90 873.9 ms; target <200 ms MISSED |
| `sh scripts/boot-direct.sh --mode full` | **exit 2**: login median 2,300.9 ms / p90 2,720.8 ms; target <1 s MISSED on this loaded host (S89 clean sample: 755/923 ms) |
| `cargo xtask qtest --case brush-login --arch x86_64` | **PASS** (226,403 ms under load) |
| `cargo xtask qtest --case release-login --arch x86_64` | **FAIL** after `raven# ls /bin`: `^cat$`/`^ls$`/`^uname$`/ping expectations unmatched (76,653 ms) |
| `sh tests/smp/ipc/run.sh kvm 4 on` / `kvm 2 on` | **PASS** (S85c; data counts exact; p90 315.6 ms / 1.091 s) |
| `sh tests/smp/lock-time/run.sh tcg` | **FAIL** at iteration 8 (S85a strict gate) |
| `sh tests/smp/memory/run.sh tcg --iterations 1` | **FAIL** (S85b, 85 timeout lines) |

Image: `build/boot-time/release.img`, 320 MiB, SHA-256
`514c7be8c205c8291c58363326f7a9db2b2a104198c5b2634420a48529b9e9d8`, banner `6807dc1f`.
Host: QEMU 6.2.0, OVMF 2022.02, KVM `kvm_intel`, i7-12700H, Linux 5.19.0-50. The host is shared;
the boot/direct samples vary with other minion QEMU load. Both outcomes are recorded; no sample is
discarded to make a pass.

## DR-52 donor reconciliation

| Topic | Illumos finding | BSD file/licence | Linux finding | Decision |
|---|---|---|---|---|
| Boot-time measurement | SMF boot phases/milestones; design-only | FreeBSD `usr.sbin/bhyve` (BSD-2) is a VMM, not a timing harness; no permissive comparator | systemd boot-phase timing/`perf`; read-only | First-party `scripts/boot-time.sh`; no donor code |
| Direct PVH boot loader | boot stages and loader handoff; design-only | FreeBSD loader/`bhyve` (BSD-2) as a reference for a first-stage loader; no code copied | PVH `hvm_start_info` behaviour; read-only | S89 first-party `boot/direct` loader; no donor code |
| SMP AP bring-up | `mp_startup`, `xcalls`, TLB ack; design-only | FreeBSD `mp_machdep.c`/NetBSD `cpu.c` (per-file BSD; reference only) | `smpboot.c`/`tlb.c`; read-only | S83 clean-room fix; no donor code |
| Stress campaigns | xcalls/turnstiles/TLB ack; design-only | NetBSD ATF (BSD-2) is the existing test-harness donor; S85 harnesses are first-party | scheduler/IPC/TLB stress outcomes; read-only | First-party `tests/smp/**`; exact failures in the S85 reports |
| Boot-phase table | boot milestones; no code | FreeBSD loader/rc phases (BSD-2) as naming reference | systemd-analyze phase split; read-only | Firmware/kernel→login/total reported separately; no borrowed numbers |
| BSD/Linux images | not applicable | FreeBSD release image would be the BSD comparator | Linux distro image would be the Linux comparator | **Blocked:** owner/reviewer must freeze names/versions/URLs/checksums; no comparison run |

No donor code was copied, translated or closely paraphrased.

## Exact blockers

| Blocker | Owner / file |
|---|---|
| TCG TLB shootdown timeouts (S85a/b, S85f aarch64) | stuart/S83, `kernel/src/smp/shootdown.rs`, `kernel/src/sched/smp_shim.rs:225` |
| `libc-threads`/malloc/stdio concurrency | otto + libc-thread/stdio lanes |
| VFS/IPC concurrency at iteration 0 | mel + stuart/tim (vfsd/ramfsd/router) |
| `release-login` external-command path under brush | phil/S96 (S109 partly landed) |
| Direct probe <200 ms and full <1 s under load | kevin/stuart, `boot/direct`, kernel early init |
| BSD/Linux frozen image set/checksums | owner/reviewer |

## Reproduction

```sh
sh scripts/qemu-kvm.sh --timeout 120
sh scripts/boot-time.sh --smp 4
sh scripts/boot-time.sh --tcg --runs 1
sh scripts/boot-direct.sh --arch x86_64
sh scripts/boot-direct.sh --mode full
cargo xtask qtest --case brush-login --arch x86_64
cargo xtask qtest --case release-login --arch x86_64
sh tests/smp/ipc/run.sh kvm 4 on        # S85c KVM
sh tests/smp/lock-time/run.sh tcg       # S85a strict gate
sh tests/smp/memory/run.sh tcg --iterations 1  # S85b
```

Verdict: **PARTIAL** for S85g's documented evidence; the S85 umbrella remains **BLOCKED**, DR-49
is not consistently met, and the BSD/Linux comparison is blocked on the owner/reviewer image set.
No release/tag claim.
