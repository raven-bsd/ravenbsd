<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# RavenBSD reliability evidence — S85g

- Date: 2026-11-18
- Product base: `a0fe2765` (origin/master)
- Status: **BLOCKED.** No release reliability claim; the S85 umbrella stays open.
- Scope: the S85 SMP reliability campaigns and the DR-49 boot/reliability benchmark. This page is
  an evidence index; the detailed measurements are in `docs/reliability/smp-reliability.md` and
  `docs/release-boot-benchmark.md`.
- Rule: measured numbers only. A missing matrix is **MISSING**, never a pass.

## Evidence matrix

| Ticket | Area | State at this tree | Evidence / blocker | Owner |
|---|---|---|---|---|
| S85a | locks/time/IPIs | **WORKING**, no committed harness or matrix | report started 2026-11-18T11:52Z; stress harness not yet in the tree | lance |
| S85b | memory/paging/TLB | **WORKING**, no committed harness or matrix | report started 11:53Z; harness not yet in the tree | mark |
| S85c | IPC/rings/tracing | **WORKING**, no committed harness or matrix | report started 04:10Z; harness not yet in the tree | tim |
| S85d | POSIX/pthread/libc | **WORKING**, no committed harness or matrix | report started 11:55Z | otto |
| S85e | VFS/filesystem | **WORKING**, no committed harness or matrix | report started 04:05Z | mel |
| S85f | aarch64/TCG | **WORKING**, no committed harness or matrix | report started 11:53Z | carl |
| S83 | KVM SMP AP bring-up | **WORKING/BLOCKED**: KVM `-smp 2/4` stops before APs | `docs/reliability/smp-reliability.md` has the exact reproduction; no login/APs | stuart |
| S84 | boot-time/service order | **WORKING**: `scripts/boot-time.sh` absent; getty/login ~60 s on the direct TCG image | no boot-time script or 3-run table yet | dave |
| S89 | direct boot <0.2 s | **todo**, no `scripts/boot-direct.sh` or report | direct path not available; no timing | kevin |
| S95 | QEMU network | **todo**, no report | release network path not verified in this ticket | bob |
| S96 | brush login/no rsh | **WORKING**, no report | login/completion/rsh change not landed | phil |

The six S85 campaigns are the required stress matrix. At this commit none has committed a harness
or a run matrix, so there is no subsystem stress evidence to aggregate. The umbrella must not
close on this tree.

## What was measured for this ticket (OBSERVED)

1. **Release image** `build/x86_64/ravenbsd-x86_64.img`, 320 MiB, SHA-256
   `0dab48c0b0c266bcc42c5c3c94e708147cb7a054558093082f1a3d761ea57e82`, built by
   `cargo xtask image --arch x86_64 --platform generic --profile release` at `a0fe2765` (exit 0).
2. **TCG `-smp 4` boot loops**: 10 runs, start→loader p50 6.506 s / p90 15.428 s / max 17.316 s;
   start→kernel banner p50 14.974 s / p90 37.362 s / max 39.445 s; `login:` at ~59.97-60.00 s in
   9/10 and not before the 60 s kill in 1/10. These are TCG numbers, not the DR-49 KVM gate.
3. **KVM `-smp 1`**: boots the kernel and services (`smp: 1 cpus online`), no `login:` within
   60 s. **KVM `-smp 2`/`-smp 4`**: both stop after `timer: 1000 Hz` before `smp: N cpus online`;
   no APs, no login. This reproduces S83's blocker.
4. **`release-login` qtest (TCG)**: 1 run, PASS, 31,932 ms total, all 9 expectations matched, no
   forbidden hits. This is a scripted login/commands/network run, not a start→login split.
5. **No BSD/Linux comparison**: no FreeBSD image is present; the host Linux qcow2 files are not a
   frozen, documented benchmark set; and Raven KVM SMP is broken, so a same-harness comparison
   cannot be run. No Linux boot numbers were measured and no numbers were borrowed.

## Blockers and owners

| Blocker | Owner / file | Needed for |
|---|---|---|
| KVM `-smp 2/4` stops at `timer: 1000 Hz`; APs never online | stuart, S83, kernel AP/SMP path | KVM `-smp 4` stress and the DR-49 gate |
| No `scripts/boot-time.sh`; getty/login ~60 s on the direct image | dave, S84, `scripts/boot-time.sh` + service order | the <1 s KVM boot gate |
| No direct-boot path/script | kevin, S89, `scripts/boot-direct.sh`, `docs/qemu-direct-boot.md` | firmware-free start→login measurement |
| Network/login completion and no-rsh not landed | bob/phil, S95/S96 | a usable brush login with network |
| No S85a-f harness or matrix | lance/mark/tim/otto/mel/carl | the SMP stress matrix |
| No frozen FreeBSD/Linux images | owner/reviewer; images not in `ravenbsd-resources` | the apples-to-apples comparison |

## Reproduction

```sh
# Release image
cargo xtask image --arch x86_64 --platform generic --profile release

# TCG -smp 4 (release-login qtest; scripted login/network)
cargo xtask qtest --case release-login --arch x86_64

# KVM SMP reproduction (must not hide the failure)
timeout 60 qemu-system-x86_64 -machine q35 -m 512M -smp 4 -enable-kvm -cpu host \
  -serial stdio -display none -monitor none -no-reboot \
  -drive if=pflash,format=raw,readonly=on,file=/usr/share/OVMF/OVMF_CODE_4M.fd \
  -drive if=pflash,format=raw,file=build/x86_64/OVMF_VARS.fd \
  -drive format=raw,file=build/x86_64/ravenbsd-x86_64.img \
  -device isa-debug-exit,iobase=0xf4,iosize=0x04
```

Verdict: **BLOCKED** on KVM SMP (S83), the boot-time/service path (S84/S89/S95/S96), the six
missing stress matrices, and the missing BSD/Linux benchmark images. No tag, release, or
reliability pass is claimed.
