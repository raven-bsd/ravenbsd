<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# SMP reliability evidence (S85g)

- Date: 2026-11-18
- Product base: `a0fe2765` (origin/master)
- Host: 12th Gen Intel(R) Core(TM) i7-12700H, Linux 5.19.0-50-generic
- QEMU: `qemu-system-x86_64` 6.2.0 (Debian 1:6.2+dfsg-2ubuntu6.31); OVMF 2022.02-3ubuntu0.22.04.7
- KVM: `/dev/kvm` present, `kvm_intel` loaded; `kvm-ok` reports acceleration usable
- Image: `build/x86_64/ravenbsd-x86_64.img`, 320 MiB, SHA-256
  `0dab48c0b0c266bcc42c5c3c94e708147cb7a054558093082f1a3d761ea57e82`, built at `a0fe2765` with
  `cargo xtask image --arch x86_64 --platform generic --profile release` (exit 0)
- Status: **BLOCKED.** No stress campaign can be aggregated at this tree.

## Required matrix vs. what exists

The S85 umbrella requires each of S85a-f to run its subsystem stress harness under TCG `-smp 4`
and, once S83 lands, KVM `-smp 2/4`, with at least 50 iterations or a documented time-box, and
worst-case/p90. At `a0fe2765`:

| Sub-ticket | Harness path | Committed matrix | State |
|---|---|---|---|
| S85a locks/time/IPIs | `tests/smp/lock-time/**` (S85a Owns) | none | report WORKING, started 11:52Z |
| S85b memory/paging/TLB | `tests/smp/memory/**` (S85b Owns) | none | report WORKING, started 11:53Z |
| S85c IPC/rings/tracing | S85c Owns | none | report WORKING, started 04:10Z |
| S85d POSIX/pthread/libc | S85d Owns | none | report WORKING, started 11:55Z |
| S85e VFS/filesystem | S85e Owns | none | report WORKING, started 04:05Z |
| S85f aarch64/TCG | S85f Owns | none | report WORKING, started 11:53Z |

`tests/smp/` does not exist in the tree. There is no campaign log, iteration count, worst-case or
p90 to quote. Reporting a pass from a one-shot boot would violate the S85 contract; none is
claimed.

## Local boot/SMP measurements (OBSERVED)

These are S85g's own measurements, useful as the harness baseline, not as the missing campaigns.

### TCG `-smp 4` release image (10 runs)

Command (per run, 60 s foreground timeout, serial timestamps taken at the QEMU output line):

```sh
timeout 60 qemu-system-x86_64 -machine q35 -m 512M -smp 4 -serial stdio -display none \
  -monitor none -no-reboot -nic none -netdev user,id=n0 \
  -device virtio-net-pci,netdev=n0,disable-legacy=on \
  -drive if=pflash,format=raw,readonly=on,file=/usr/share/OVMF/OVMF_CODE_4M.fd \
  -drive if=pflash,format=raw,file=build/x86_64/OVMF_VARS.fd \
  -drive format=raw,file=build/x86_64/ravenbsd-x86_64.img \
  -device isa-debug-exit,iobase=0xf4,iosize=0x04
```

| Run | start→loader (s) | start→kernel banner (s) | start→`login:` (s) |
|---:|---:|---:|---:|
| 1 | 4.026 | 9.699 | 59.990 |
| 2 | 7.099 | 14.514 | 59.993 |
| 3 | 12.053 | 26.362 | 59.980 |
| 4 | 5.913 | 15.435 | 60.001 |
| 5 | 14.596 | 39.445 | 59.983 |
| 6 | 17.316 | 35.882 | not before 60 s |
| 7 | 5.780 | 14.466 | 59.965 |
| 8 | 15.428 | 37.362 | 59.973 |
| 9 | 4.280 | 9.466 | 59.995 |
| 10 | 4.220 | 9.490 | 59.997 |
| **p50** | **6.506** | **14.974** | — |
| **p90** | **15.428** | **37.362** | — |
| **max** | **17.316** | **39.445** | — |

`login:` at ~59.97-60.00 s in 9/10 runs (immediately before the 60 s kill) and absent in run 6;
the services all report up, but the getty/login path is delayed. The qtest `release-login`
(scripted login + commands + ping) passes in 31,932 ms total; it is not a start→login split and
uses the case's own image/cmdline, so it is not substituted for the table above.

### KVM SMP reproduction

```sh
# Same command with -enable-kvm -cpu host and -smp 1|2|4
```

| KVM run | Result | Last serial evidence |
|---|---|---|
| `-smp 1` | kernel + services up; **no `login:` in 60 s** | `smp: 1 cpus online`; getty/services later; timeout 124 |
| `-smp 2` | **stops before APs** | last line `timer: 1000 Hz`; no `smp: 2 cpus online`; QEMU exits 0 |
| `-smp 4` | **stops before APs** | last line `timer: 1000 Hz`; no `smp: 4 cpus online`; QEMU exits 0 |

This reproduces the S83 report: the AP bring-up path fails under KVM before the APs are online, so
KVM `-smp 2/4` stress and the DR-49 KVM gate cannot run. S83 owns the fix; the exact path is the
kernel SMP/AP bring-up, not this ticket's Owns.

## Classification and owners

| Finding | Class | Owner |
|---|---|---|
| KVM `-smp 2/4` stops at `timer: 1000 Hz`, APs never online | blocker | stuart, S83 |
| Direct release image needs ~60 s to print `login:`; `scripts/boot-time.sh` absent | blocker | dave, S84 |
| No direct-boot path/script | blocker | kevin, S89 |
| Network/login/no-rsh path not landed | blocker | bob/phil, S95/S96 |
| No S85a-f stress harness/matrix | missing evidence | lance/mark/tim/otto/mel/carl |
| No frozen BSD/Linux benchmark images | missing evidence | owner/reviewer |

A known flake or an unexplained failure is a blocker; none is retried away. There is no
"eventually passed" row in this document.

## Reproduction

The image build and the qtest command are in `docs/reliability/README.md`. The KVM command is the
same QEMU line with `-enable-kvm -cpu host` and `-smp 1`, `-smp 2` or `-smp 4`; capture the serial
log and the exit status, never a retry that hides the stop. The full logs for these runs are under
`/tmp/opencode/s85g-*.log` on the measuring host (not committed).

Verdict: **BLOCKED** — KVM SMP is broken, the six stress campaigns are not available, and the
release boot path is not yet at the measured gate. No reliability pass is claimed.
