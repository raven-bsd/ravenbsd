<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# RavenBSD release boot benchmark (DR-49) — rework round 3

- Date: 2026-11-18
- Product base: `6807dc1f` (origin/master `b6163e85` merged into `jerry/S85g`)
- Status: **PARTIAL / DR-49 NOT consistently met.** Raven-side data is current; the full-boot
  gate is load-sensitive and the BSD/Linux comparison is blocked on the owner/reviewer image set.
  No marketing claim.
- Rule: measured numbers only, same harness, no borrowed numbers.

## What DR-49 requires

Raven must reach a usable brush login, with completion and working network (not rsh), in **under
one second including firmware**, under **KVM `-smp 4`**, and beat BSD/Linux in the **same**
QEMU/KVM harness. TCG is reported, not gated.

## Environment

| Item | Value |
|---|---|
| Host | i7-12700H, Linux 5.19.0-50-generic |
| QEMU | 6.2.0 (Debian 1:6.2+dfsg-2ubuntu6.31) |
| Firmware | OVMF 2022.02-3ubuntu0.22.04.7 |
| KVM | `/dev/kvm`, `kvm_intel`; aarch64 KVM unavailable in this QEMU build |
| Raven commit | `6807dc1f` (origin/master `b6163e85` merged) |
| Raven image | `build/boot-time/release.img`, 320 MiB, SHA-256 `514c7be8c205c8291c58363326f7a9db2b2a104198c5b2634420a48529b9e9d8` |
| Image command | `sh scripts/boot-time.sh --smp 4` builds from HEAD and verifies the banner commit |

## Raven measurements (OBSERVED)

### KVM SMP path — PASS

`sh scripts/qemu-kvm.sh --timeout 120`:

```
qemu-kvm: PASS -smp 1
qemu-kvm: PASS -smp 2
qemu-kvm: PASS -smp 4
qemu-kvm: PASSED -smp 1,2,4 iterations=1
```

### UEFI/OVMF boot split (KVM 4 CPU, 3 runs)

| Run | firmware (ms) | kernel→login (ms) | total (ms) |
|---|---:|---:|---:|
| 1 | 4,135 | 382 | 4,518 |
| 2 | 6,796 | 636 | 7,433 |
| 3 | 7,489 | 694 | 8,183 |
| median | 6,796 | **636** | 7,433 |
| p90 | 7,489 | **694** | 8,183 |

OS gate **PASS** (kernel→login 636 ms < 1 s). S84 reference at `4ceb76b3`: 409 ms median / 425 ms
p90. The **full QEMU start→login is 4.5-8.2 s** because OVMF dominates, so the full <1 s gate
**fails on the UEFI path**. Earlier samples on the same shared host ranged up to 19.1 s firmware
and a 1,532 ms kernel segment; the host load is real and both outcomes are kept.

TCG `sh scripts/boot-time.sh --tcg --runs 1`: kernel→login 24,663 ms, total 65,244 ms (report
only; host loaded).

### Direct boot (S89)

`sh scripts/boot-direct.sh --arch x86_64` (microvm, KVM, release initfs, 3 runs): exit 2,
kernel-banner median **571.7 ms** / p90 **873.9 ms**; target <200 ms **MISSED** (S89 clean sample
was 348 ms).

`sh scripts/boot-direct.sh --mode full` (q35, KVM 4, virtio-net, release initfs, 3 runs): exit 2,
login median **2,300.9 ms** / p90 **2,720.8 ms**; target <1 s **MISSED** on this loaded host.
S89's clean sample was login median **755 ms** / p90 **923 ms (MET)**; its loaded
service-asserting sample was 1,080 ms. The direct full gate is therefore **marginal and
load-sensitive**, not a clean release pass. The loader, getty, netd and the virtio network were up
in all the current runs.

### Login shell and network

- `cargo xtask qtest --case brush-login --arch x86_64`: **PASS** — the login shell is brush,
  completion is registered, no `rsh: ready`; 226,403 ms under load.
- `cargo xtask qtest --case release-login --arch x86_64`: **FAIL** after `raven# ls /bin`; the
  external-command output (`cat`/`ls`/`uname`/ping) is not produced within 30 s (76,653 ms).
  The brush identity/completion and the network services are up, but the usable external-command
  login path remains the S96/S109 blocker. No "working network at a usable login" PASS is claimed.

### BSD/Linux comparison — exact blocker

- **No frozen BSD or Linux image set exists in the repo.** The owner/reviewer must supply the
  exact images with names, versions, download URLs and SHA-256 checksums. Unrelated host qcow2
  files are not a documented benchmark set and were **not used**.
- **No Linux or BSD boot number is measured** in this ticket; none is borrowed.
- The comparison cannot start until the Raven direct full-boot result is stable on the same
  harness and the frozen comparator images are available.

## DR-52 donor reconciliation

| Topic | Illumos finding | BSD file/licence | Linux finding | Decision |
|---|---|---|---|---|
| Boot-time measurement | SMF boot phases/milestones; design-only | FreeBSD `usr.sbin/bhyve` (BSD-2) is a VMM, not a timing harness; no permissive comparator | systemd boot-phase timing/`perf`; read-only | First-party `scripts/boot-time.sh`; no donor code |
| Direct PVH loader | boot stages and loader handoff; design-only | FreeBSD loader/`bhyve` (BSD-2) as a reference; no code copied | PVH `hvm_start_info` behaviour; read-only | S89 first-party `boot/direct` loader; no donor code |
| SMP AP bring-up | `mp_startup`, `xcalls`, TLB ack; design-only | FreeBSD `mp_machdep.c`/NetBSD `cpu.c` (per-file BSD; reference only) | `smpboot.c`/`tlb.c`; read-only | S83 clean-room fix; no donor code |
| Boot-phase table | boot milestones; no code | FreeBSD loader/rc phases (BSD-2) as naming reference | systemd-analyze phase split; read-only | Firmware/kernel→login/total reported separately; no borrowed numbers |
| BSD/Linux comparator images | not applicable | FreeBSD release image would be the BSD comparator | Linux distro image would be the Linux comparator | **Blocked:** owner/reviewer must freeze names/versions/URLs/checksums; no comparison yet |

No donor source was copied or translated for this benchmark.

## What closes the benchmark

1. **S96/S109**: make external commands work under the brush login (`release-login` green) so the
   "usable login with network" claim can be measured.
2. **S89**: reduce the direct full-boot time or stabilize it on an unloaded host; the clean sample
   met <1 s, the current loaded sample did not. The probe <200 ms target also needs the kernel
   early-init share reduced.
3. **S84/host**: keep the OS gate sample on an unloaded host; the kernel→login 636 ms pass and the
   409 ms reference are the current anchors, but the host-load flake is recorded.
4. **Owner/reviewer**: freeze the exact BSD and Linux images (names, versions, URLs, SHA-256) and
   the identical QEMU/KVM command line; then run the same harness and report median/worst case.
5. **S85a-f**: the stress matrix must be green; S85c is green, the other five lanes are still
   BLOCKED (`docs/reliability/smp-reliability.md`).

## Reproduction

```sh
sh scripts/qemu-kvm.sh --timeout 120
sh scripts/boot-time.sh --smp 4
sh scripts/boot-time.sh --tcg --runs 1
sh scripts/boot-direct.sh --arch x86_64
sh scripts/boot-direct.sh --mode full
cargo xtask qtest --case brush-login --arch x86_64
cargo xtask qtest --case release-login --arch x86_64
```

Verdict: **PARTIAL** for the documented Raven-side evidence; **DR-49 NOT consistently met**
(full UEFI >1 s; direct full 755 ms clean vs 2,301 ms loaded; probe <200 ms missed; release-login
external commands fail). The BSD/Linux comparison is blocked on the owner/reviewer image set. No
tag or release PASS is claimed.
