<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# RavenBSD release boot benchmark (DR-49) — rework round 4

- Date: 2026-11-18
- Product base: `eb3ac161` (origin/master merged into `jerry/S85g` as `45209b77`)
- Measured samples: `6807dc1f` (round 3, shared host) and `eb3ac161` (release-login, round 4)
- Status: **DONE for this ticket's evidence; DR-49 not consistently met.** Raven-side numbers are
  kept with their commit and host state; cross-OS benchmarking is **ravenmark's** deliverable
  (DR-53), not this ticket's. No marketing claim.
- Rule: measured numbers only, same harness, no borrowed numbers, no new benchmark run in round 4
  (host saturated).

## What DR-49 requires

Raven must reach a usable brush login, with completion and working network (not rsh), in **under
one second including firmware**, under **KVM `-smp 4`**, and beat BSD/Linux in the **same**
QEMU/KVM harness. The cross-OS comparison is owned by **ravenmark** (DR-53); S85g keeps the
Raven-side evidence and does not duplicate the baseline.

## Environment

| Item | Value |
|---|---|
| Host | i7-12700H, Linux 5.19.0-50-generic (shared/saturated) |
| QEMU | 6.2.0 (Debian 1:6.2+dfsg-2ubuntu6.31) |
| Firmware | OVMF 2022.02-3ubuntu0.22.04.7 |
| KVM | `/dev/kvm`, `kvm_intel`; aarch64 KVM unavailable in this QEMU build |
| Measured sample commit | `6807dc1f` (round 3); release-login at `eb3ac161` (round 4) |
| Image | `build/boot-time/release.img`, 320 MiB, SHA-256 `514c7be8c205c8291c58363326f7a9db2b2a104198c5b2634420a48529b9e9d8` (round 3) |

## Raven measurements (OBSERVED, commit/host labelled)

### KVM SMP path — PASS (`6807dc1f`, shared host)

`sh scripts/qemu-kvm.sh --timeout 120`:

```
qemu-kvm: PASS -smp 1
qemu-kvm: PASS -smp 2
qemu-kvm: PASS -smp 4
qemu-kvm: PASSED -smp 1,2,4 iterations=1
```

### UEFI/OVMF boot split (`6807dc1f`, shared host, 3 runs)

| Run | firmware (ms) | kernel→login (ms) | total (ms) |
|---|---:|---:|---:|
| 1 | 4,135 | 382 | 4,518 |
| 2 | 6,796 | 636 | 7,433 |
| 3 | 7,489 | 694 | 8,183 |
| median | 6,796 | **636** | 7,433 |
| p90 | 7,489 | **694** | 8,183 |

OS gate **PASS** (kernel→login 636 ms < 1 s). S84 reference at `4ceb76b3`: 409 ms median / 425 ms
p90. The **full UEFI start→login is 4.5-8.2 s** (OVMF dominates), so the full <1 s gate **fails on
the UEFI path**.

TCG `--tcg --runs 1` at `6807dc1f` (loaded host): kernel→login 24,663 ms; total 65,244 ms (report
only).

### Direct boot (S89, `6807dc1f`)

`sh scripts/boot-direct.sh --arch x86_64` (microvm, KVM, 3 runs): exit 2, kernel median
**571.7 ms** / p90 **873.9 ms**; target <200 ms **MISSED** (S89 clean sample: 348 ms).

`sh scripts/boot-direct.sh --mode full` (q35, KVM 4, virtio-net, 3 runs, **loaded host**): exit 2,
login median **2,300.9 ms** / p90 **2,720.8 ms**; target <1 s **MISSED**. S89's clean sample is
login median **755 ms** / p90 **923 ms (MET)**; its loaded service-asserting sample is 1,080 ms.
The direct full gate is **marginal and load-sensitive**; the loaded sample is labelled and not
presented as a clean pass.

### Login shell and network

- `cargo xtask qtest --case brush-login --arch x86_64`: **PASS** at `6807dc1f` (brush identity,
  completion, no rsh; 226,403 ms under load).
- `cargo xtask qtest --case release-login --arch x86_64`: **PASS at `eb3ac161`** (36,952 ms).
  The earlier round-3 "external commands fail" claim was **stale**; S96/S109 landed. The release
  login path, brush shell and network are usable in this gate.

## Cross-OS benchmark — ravenmark, not S85g

Per **DR-53**, ravenmark owns the Illumos/BSD/Linux baseline: exact images, versions, URLs,
checksums and the identical QEMU/KVM command line. S85g does **not** run or publish the comparison;
there is no invented or borrowed Linux/BSD number here. The cross-OS row is therefore a pointer:

| Baseline | Owner | State |
|---|---|---|
| Illumos / BSD / Linux comparison | **ravenmark** (DR-53) | owned there; S85g does not duplicate it |

## DR-52 donor reconciliation

| Topic | Illumos finding | BSD file/licence | Linux finding | Decision |
|---|---|---|---|---|
| Boot-time measurement | SMF boot phases/milestones; design-only | FreeBSD `usr.sbin/bhyve` (BSD-2) is a VMM, not a timing harness; no permissive comparator | systemd boot-phase timing/`perf`; read-only | First-party `scripts/boot-time.sh`; no donor code |
| Direct PVH loader | boot stages/loader handoff; design-only | FreeBSD loader/`bhyve` (BSD-2) as a reference; no copy | PVH `hvm_start_info`; read-only | S89 first-party `boot/direct` loader; no donor code |
| SMP AP bring-up | `mp_startup`, `xcalls`, TLB ack; design-only | FreeBSD `mp_machdep.c`/NetBSD `cpu.c` (per-file BSD; reference only) | `smpboot.c`/`tlb.c`; read-only | S83 clean-room fix; no donor code |
| Boot-phase table | boot milestones; no code | FreeBSD loader/rc phases (BSD-2) as naming reference | systemd-analyze phase split; read-only | Firmware/kernel→login/total separately; no borrowed numbers |
| Cross-OS baseline | ravenmark's comparison | ravenmark owns the BSD image set | ravenmark owns the Linux image set | **DR-53:** ravenmark owns it; S85g does not duplicate |

No donor source was copied or translated for this benchmark.

## What remains (not benchmark runs in this ticket)

1. **S89/S84**: the direct full-boot result is 755 ms clean but 2,301 ms on the loaded host, and
   the probe <200 ms misses; stabilize on an unloaded host or reduce kernel early init.
2. **S113/S108/S110/S119**: the remaining reliability blocks (SMP TLB IPI, scheme IPC/VFS, disk
   tools/UFS, libc stdio). The umbrella stays open until they are green.
3. **ravenmark**: the Illumos/BSD/Linux comparison with a frozen image set and the identical
   harness; S85g points to it and does not duplicate it.

## Reproduction (evidence only; no new benchmark run in round 4)

```sh
cargo xtask qtest --case release-login --arch x86_64   # PASS on eb3ac161
sh scripts/qemu-kvm.sh --timeout 120                   # measured at 6807dc1f
sh scripts/boot-time.sh --smp 4                        # measured at 6807dc1f
sh scripts/boot-direct.sh --mode full                  # loaded-host sample at 6807dc1f
```

Verdict: **DONE for S85g's Raven-side evidence/docs; DR-49 not consistently met** (full UEFI
>1 s; direct full 755 ms clean vs 2,301 ms loaded; probe <200 ms missed). Cross-OS benchmarking is
ravenmark's (DR-53); the S85 umbrella remains blocked on S113/S108/S110/S119. No tag or release
PASS is claimed.
