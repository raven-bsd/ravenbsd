<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# RavenBSD release boot benchmark (DR-49) — rework round 2

- Date: 2026-11-18
- Product base: `6ff7c106` (origin/master `94648948` merged into `jerry/S85g`)
- Status: **PARTIAL / DR-49 NOT MET.** The Raven-side is measured on the current tree; the
  BSD/Linux comparison is blocked on the owner/reviewer image set. No marketing claim.
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
| Raven commit | `6ff7c106` (origin/master `94648948` merged) |
| Raven image | `build/boot-time/release.img`, 320 MiB, SHA-256 `7987e189325dc55208fb8da4966faa62ffa2ea035bcd8bc6dc16eb4f412578f9` |
| Image command | `sh scripts/boot-time.sh --smp 4` builds from HEAD and verifies the banner commit |

## Raven measurements (OBSERVED)

### KVM SMP path — PASS

`sh scripts/qemu-kvm.sh --no-build --image build/boot-time/release.img --timeout 120`:

```
qemu-kvm: PASS -smp 1
qemu-kvm: PASS -smp 2
qemu-kvm: PASS -smp 4
qemu-kvm: PASSED -smp 1/2/4 with login
```

### Boot-time split (KVM 4 CPU, 3 runs per sample)

| Sample | firmware (ms) | kernel→login (ms) | total (ms) | Gate |
|---|---|---:|---:|---|
| A | 8,489 / 19,114 / 10,474 | 1,085 / 2,021 / 1,532 | 9,575 / 21,135 / 12,007 | **FAIL** (median 1,532 ms) |
| B | 10,500 / 6,648 / 5,086 | 1,081 / 534 / 426 | 11,582 / 7,182 / 5,512 | **PASS** (median 534 ms, p90 1,081 ms) |

Committed S84 table at `4ceb76b3`: kernel→login median **409 ms**, p90 425 ms; firmware ~4.34 s.
The two fresh samples vary by ~3× on the kernel segment and ~4× on firmware while other minion
QEMUs ran on the host. The OS gate (kernel→login < 1 s) is therefore **flaky** on this host, not
a clean pass; the full QEMU start→login is 5.5-21.1 s because OVMF dominates.

TCG `sh scripts/boot-time.sh --tcg --runs 1`: kernel→login 3,689 ms, total 12,362 ms (report
only, no gate).

### Direct boot — BLOCKED

`sh scripts/boot-direct.sh --arch x86_64` exits 2:

```
qemu-system-x86_64: Error loading uncompressed kernel without PVH ELF Note
BLOCKED: the kernel ELF has no PVH note (XEN_ELFNOTE_PHYS32_ENTRY)
required: a 32-bit PVH entry ... or a first-stage multiboot/PVH loader.
owner: Stuart (kernel entry/S83 lane)
```

No firmware-free number exists; the full <1 s including firmware gate **fails**. The brush login
and network path itself is green (`qemu-kvm` login; S84/S95 reports; `release-login` in the 27-case
TCG kernel gate).

### BSD/Linux comparison — exact blocker

- **No frozen BSD or Linux image set exists in the repo.** The owner/reviewer must supply the
  exact images with names, versions, download URLs and SHA-256 checksums; the host has some
  unrelated qcow2 files, but they are not a documented benchmark set and were **not used**.
- **No Linux or BSD boot number is measured** in this ticket; none is borrowed from a vendor.
- The comparison cannot start until the KVM Raven side is stable on the same harness (the boot
  samples above are flaky) and S89 removes the firmware path (or the owner explicitly redefines
  the full-boot gate).

## DR-52 donor reconciliation

| Topic | Illumos finding | BSD file/licence | Linux finding | Decision |
|---|---|---|---|---|
| Boot-time measurement harness | SMF boot phases/milestones (`boot`, `svc.startd`); design-only | FreeBSD `usr.sbin/bhyve` (BSD-2) is a VMM, not a timing harness; no permissive boot-time comparator | systemd boot-phase timing (`systemd-analyze`) and `perf` outcomes; read-only | First-party `scripts/boot-time.sh` with QEMU/serial markers; no donor code |
| Boot-phase split (firmware/kernel/login) | boot stages and milestones; no code | FreeBSD loader/rc phases (BSD-2) as naming reference | systemd-analyze phase split; read-only | First-principles markers; firmware, kernel→login and total reported separately |
| KVM SMP AP bring-up | `mp_startup`, `xcalls`, TLB ack; design-only | FreeBSD `mp_machdep.c`/NetBSD `cpu.c` (per-file BSD; reference only) inform behaviour; S83 is clean-room | `smpboot.c`/`tlb.c` outcomes; read-only | S83 clean-room kernel fix; no donor code; see S83 |
| BSD/Linux comparator images | not applicable | FreeBSD release image would be the BSD comparator | Linux distro image would be the Linux comparator | **Blocked:** owner/reviewer must freeze the image set/checksums; no comparison yet |

No donor source was copied or translated for this benchmark.

## What closes the benchmark

1. **S89**: add the PVH/multiboot entry (owner Stuart) or an approved first-stage loader; then
   measure direct QEMU start → brush login on KVM `-smp 4`.
2. **S84/host**: get a stable 3-run KVM gate on an unloaded host (the committed 409 ms and the
   fresh 534 ms pass are the references; the 1,532 ms sample is a measured flake).
3. **Owner/reviewer**: freeze the exact BSD and Linux images (names, versions, URLs, SHA-256) and
   the identical QEMU/KVM command line; then run the same harness and report median/worst case.
4. **S85a-f**: the stress matrix must be green before a release reliability claim; its current
   failures are in `docs/reliability/smp-reliability.md`.

## Reproduction

```sh
sh scripts/boot-time.sh --smp 4
sh scripts/qemu-kvm.sh --no-build --image build/boot-time/release.img --timeout 120
sh scripts/boot-direct.sh --arch x86_64
sh scripts/boot-time.sh --tcg --runs 1
```

Verdict: **PARTIAL** for the documented Raven-side evidence; **DR-49 NOT MET** (full boot
including firmware > 1 s; direct path blocked; OS gate flaky on this host). The BSD/Linux
comparison is blocked on the owner/reviewer image set. No tag or release PASS is claimed.
