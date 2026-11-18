<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# RavenBSD release boot benchmark (DR-49)

- Date: 2026-11-18
- Product base: `a0fe2765` (origin/master)
- Status: **BLOCKED / NOT VERIFIABLE.** No Raven-vs-BSD/Linux claim is made; DR-49 is not met
  on this tree.
- Rule: measured numbers only, same harness, no borrowed numbers, no marketing.

## What DR-49 requires

Raven must reach a usable brush login, with completion and working network (not rsh), in **under
one second including firmware**, under **KVM `-smp 4`**, and must beat BSD/Linux in the **same**
QEMU/KVM harness. TCG is reported, not gated (S84 contract).

## Environment (exact)

| Item | Value |
|---|---|
| Host CPU | 12th Gen Intel(R) Core(TM) i7-12700H |
| Host kernel | 5.19.0-50-generic |
| QEMU | `qemu-system-x86_64` 6.2.0 (Debian 1:6.2+dfsg-2ubuntu6.31) |
| Firmware | OVMF 2022.02-3ubuntu0.22.04.7 (`/usr/share/OVMF/OVMF_CODE_4M.fd`) |
| KVM | `/dev/kvm` present; `kvm_intel` loaded; `kvm-ok` acceleration usable |
| Raven commit | `a0fe2765` |
| Raven image | `build/x86_64/ravenbsd-x86_64.img`, 320 MiB, SHA-256 `0dab48c0b0c266bcc42c5c3c94e708147cb7a054558093082f1a3d761ea57e82` |
| Image command | `cargo xtask image --arch x86_64 --platform generic --profile release` (exit 0) |

## Raven measurements (OBSERVED)

### KVM (the DR-49 harness) — BLOCKED

| Run | Result |
|---|---|
| KVM `-smp 1` | kernel + services up (`smp: 1 cpus online`); **no `login:` within 60 s**; timeout |
| KVM `-smp 2` | **stops at `timer: 1000 Hz`**; no `smp: 2 cpus online`; no login; QEMU exit 0 |
| KVM `-smp 4` | **stops at `timer: 1000 Hz`**; no `smp: 4 cpus online`; no login; QEMU exit 0 |

The KVM SMP failure is the S83 AP bring-up blocker (reproduced in
`docs/reliability/smp-reliability.md`). There is no KVM boot-to-login number to compare, so the
DR-49 gate is **not measurable**.

### TCG `-smp 4` baseline (reported, not gated)

10 runs, release image with the qtest network device (details/table in
`docs/reliability/smp-reliability.md`):

| Metric | p50 | p90 | max |
|---|---:|---:|---:|
| start → loader | 6.506 s | 15.428 s | 17.316 s |
| start → kernel banner | 14.974 s | 37.362 s | 39.445 s |
| start → `login:` | ~59.97 s (9/10; 1/10 not before 60 s) | — | — |

The getty/login delay on the direct release image is the S84 service-ordering issue. The
`release-login` qtest (scripted login, `ls`, `uname`, `ping 10.0.2.2`, exit 0) passes once in
**31,932 ms total**; it is not a start→login split and is not substituted for the table.

### BSD/Linux comparison — BLOCKED

- **No FreeBSD image** is present in the workspace or the searched host paths; no BSD boot number
  exists.
- Candidate host Linux qcow2 files (not a frozen, documented benchmark set):
  - `/home/bo/Downloads/openSUSE-Leap-Micro.x86_64-Default-qcow.qcow2`, virtual 20 GiB,
    SHA-256 `a276909e1ef16260b82f172ba8e990e917eb3f214b762c826f8c18fca40a9015`
  - `/home/bo/Downloads/openSUSE-MicroOS.x86_64-ContainerHost-kvm-and-xen.qcow2`, virtual 20 GiB,
    SHA-256 `1feea5640c8298f6296c057af07ec94179195a922b25191c16deb79739c44126`
  - `/home/bo/Downloads/qemu_minimal_image.x86_64-1.1.0-0.qcow2`, virtual 3 GiB,
    SHA-256 `17bbc74ba30921e89376e8c96e9288405e9ab756f28e8bab6eb8a2ac9ea69b0d`
  - `/home/bo/Downloads/basic-image.aarch64-1.0.2-RW-Build1.qcow2`, virtual 580 MiB,
    SHA-256 `6a9b842c343386944db864d06a4e2f799e76800d6ba95d72b7c181e89b680a14` (not x86_64)
  These are not used for a comparison: the Raven KVM side is broken, and the images have no
  frozen provenance (source URL/version) or identical-harness contract.
- **No Linux boot time was measured** in this ticket. No borrowed or vendor-quoted number is
  used.

## What closes the benchmark

1. **S83** fixes KVM `-smp 2/4` AP bring-up; re-run the exact KVM commands with the release image.
2. **S84/S89/S95/S96** provide `scripts/boot-time.sh`, the direct-boot path, working network and
   the brush/no-rsh login; measure QEMU start → brush login (median of 3, plus worst case) and the
   S89 sub-metrics.
3. **Freeze a BSD/Linux image set** with exact names, versions, download URLs and SHA-256 values,
   and run the same QEMU/KVM command line (CPU, memory, SMP, firmware, disk, network) against each.
4. Record at least the median and worst case for each system; the DR-49 comparison is valid only
   on one harness with one dataset.

## Reproduction

```sh
cargo xtask image --arch x86_64 --platform generic --profile release
cargo xtask qtest --case release-login --arch x86_64   # TCG scripted path
# KVM SMP: same QEMU line as docs/reliability/README.md with
#   -enable-kvm -cpu host -smp 1|2|4
```

Verdict: **BLOCKED.** DR-49 (<1 s KVM boot) is not measured; the KVM SMP path, the boot-time
service path, the direct-boot path and the frozen BSD/Linux images are missing. No Raven-vs-BSD/
Linux claim, no release PASS and no tag are made from this document.
