<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# Release host evidence (EVID6)

Assembled 2026-11-21 from merged evidence at base `origin/master` `604a9367`. These are the
host rows that the generated release packet cannot collect itself. The source ticket reports in
`ravenbsd-prjmgmt/tickets/<lane>/reports/` remain the primary record; this directory duplicates
the small durable artifacts and records the exact hash of every referenced file.

The Pi 4 board stays **HARDWARE/UNVERIFIED**: there is no board serial transcript or photo, and a
host image or QEMU run is not a board result.

## Rows

| Row | Source ticket | Source commit | Status | Command | Artifact | SHA-256 |
|---|---|---|---|---|---|---|
| shell-hygiene | S97 (quiet child exit), K133 (ls latency), K134 → K136/K148/K158 (du/TLS) | `08f43b9a`, `90729b09`, `b0cfde8b`; fresh at `604a9367` | **PASS** | `timeout 240 cargo xtask qtest --case base-tools-du --arch x86_64` (exit 0); `grep -c 'process: .* exited'` = 0 | `tests/evidence/release-host/qtest/base-tools-du.log` | `544cad327bb5597d4a807496978c7e173dd5aa9df9bb8b1430880635d73c0e53` |
| shell-hygiene / ls | K133 | `90729b09` | **PASS with host-load gap** | `timeout 90 cargo xtask qtest --case base-tools-ls-latency --arch x86_64` (K133 report, 43.7 s guest, pass); fresh rerun below failed at boot | `docs/demo-ls-latency.md` | `cf5bdc22b9221f3b31cefd04ac160f795877ab26284e7ddb97510ed087986398` |
| kernel-gate | S140 | `f803502a` | **PASS** | `timeout 620 sh scripts/kernel-gate.sh`; 27/27, x86_64 22/22, aarch64 5/5 | `docs/kernel-gate.md` | `17a95df16ed10320d43e41a5a11c27ec81458400198b128b7dc639f50bc10c46` |
| posix | POSIX gate (REL6/bob) | stamp in `docs/posix/CONFORMANCE.md` | **PASS** | `cargo xtask posix-check --profile posix --arch x86_64`; 1708 pass, 0 fail, 105 skip | `docs/posix/CONFORMANCE.md` | `19b667bbb53fdb5da94252c215902e19f43232a4d65a1dffb106cfedac93acc8` |
| smp-soak | K368 | `2e8a873f` (`8798c071`) | **PASS** | `timeout 1200 sh scripts/smp-boot-soak.sh --kvm1 10 --kvm2 10 --kvm4 10 --tcg4 10 --max-load 100 --wait-for-idle 300 --out build/smp-boot-soak/k368-wait`; 40/40 | `tests/evidence/release-smp/SHA256SUMS` | `7a46b7d2558183670a9761b8b4f0bc172bbda746ac73d5ed8529b69468429f75` |
| tcp-soak | K365 | `60b2bf51` (`0a674aeb`) | **PASS** | `timeout 700 cargo xtask qtest --case tcp-soak-kvm2 --arch x86_64`; `timeout 700 ... tcp-soak-kvm4 ...`; `timeout 1000 ... tcp-soak-tcg4 ...`; KVM2/KVM4/TCG4 8/8 cycles | `tests/evidence/tcp/soak/SHA256SUMS` | `753af19d69043fb94c01432fd4d84233fa2c6a5f2545dea4bbded8aa3ffb09d7` |
| image-x86_64 | REL6 image evidence | `fd394ff3` | **PINNED** | `docs/release-images.md` records the image hash and product commit | `docs/release-images.md` | `cf59ad921923f06d0914d6d3951707189dba2d257c1ae3ff804d351fe56bfc69` |
| image-aarch64 | REL6 image evidence | `fd394ff3` | **PINNED** | `docs/release-images.md` records the image hash and product commit | `docs/release-images.md` | `cf59ad921923f06d0914d6d3951707189dba2d257c1ae3ff804d351fe56bfc69` |
| cold-start | K135/K139 | `fd394ff3` (before state) | **PENDING K139** | `cargo xtask qtest --case login-latency` passes, but the case keeps a provisional 2 s shell cap; no 5/5 <1 s sample exists | `docs/releases/0.1.0-notes.md` | `88e6fcd36fe668c19f46489ad5245d07fc17f8636a17870322eeb768ab8074c9` |
| pi4-board | owner hardware | - | **HARDWARE/UNVERIFIED** | no board log; `sh scripts/board-pi4-check.sh --log` has no input | none | - |

## Fresh host runs at `604a9367`

### `base-tools-du` — PASS (exit 0, 102,338 ms)

```sh
timeout 240 cargo xtask qtest --case base-tools-du --arch x86_64
```

```text
base-tools-du: pass
raven# du /tmp/du
1	/tmp/du
raven# echo $?
0
raven# exit 0
init exited (0)
```

`grep -c 'process: .* exited' build/qtest/base-tools-du.log` returned `0`;
`grep -Ei 'fatal runtime|failed to spawn|panic|EXCEPTION'` found nothing. This is the S97 quiet
child-exit behaviour and the K134/K136/K148 `du` thread/TLS path on the current merged base.
The QEMU status `33` is the expected `isa-debug-exit` success status.

### `base-tools-ls-latency` — FAIL at boot (exit 1, 71,844 ms)

```sh
timeout 240 cargo xtask qtest --case base-tools-ls-latency --arch x86_64
```

```text
base-tools-ls-latency: fail
step 1 (^raven# $) failed: no matching serial line within 60 s
```

The serial stops at `sched: running (4 cpus, slice 10 ms)`; no service or shell prompt appears
and **no `ls` command is reached**. At the run time the host was at load average **80 on 20 CPUs**
with four other QEMU guests and concurrent `cargo nextest`/`brush` builds, which is the recorded
environment cause. This is preserved as a host-load gap, not hidden as a pass and not used to
reopen K133. The K133 fix itself is the merged `90729b09` change and its recorded 43.7 s pass.

## Referenced source reports (outside the product tree)

| Ticket | Report path | SHA-256 |
|---|---|---|
| S97 | `ravenbsd-prjmgmt/tickets/stuart/reports/S97.md` | `d52c0e78492bec1d48807864ea826054bda1ea0f7ca27c7fd93af5c71113ad07` |
| K133 | `ravenbsd-prjmgmt/tickets/mel/reports/K133.md` | `7e130dc552eda11919a4e4ac8f5dcdbac180b0bb8d609698f62b62ea12b05dcd` |
| K134 | `ravenbsd-prjmgmt/tickets/tim/reports/K134.md` | `e555e3d3c73759b831a3ccee6dbfb145c93e2e9d67197a6825340a203654856a` |
| K368 | `ravenbsd-prjmgmt/tickets/stuart/reports/K368.md` | `2f369b8fe99991acd60aac12a145396a34ffc537b9b6dba014e2e0b87d1d06e4` |
| K365 | `ravenbsd-prjmgmt/tickets/stuart/reports/K365.md` | `4c5b2fa3feaba2fe75f32da48b24805ebba29d17af017922ce57d6a9a5aba93b` |

## Verification

```sh
timeout 60 sh tests/evidence/release-host/check.sh
```

The check runs `sha256sum -c tests/evidence/release-host/SHA256SUMS`, verifies the nested K368 and
K365 manifests, and requires the packet to carry the `shell-hygiene`, `tcp-soak` and
`HARDWARE/UNVERIFIED` rows. It does not run QEMU.
