<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# aarch64 cold-start login baseline (K145)

Status: **BLOCKED at the login stage.** The aarch64 `release` image boots under QEMU `virt`,
starts getty and prints `login:`, but the `login` child exits with status 1 after the username is
sent and **before** `Password:`; `init exited (1)`. No cold-start samples exist, so no timing is
claimed. The harness and the exact failing stage are preserved here.

## 1. Image and QEMU

* Profile: `release` (which stages the dynamic `/bin` userland, `/bin/getty`, `/bin/login`, the
  accounts and `/etc`), built for the generic aarch64 QEMU virt platform:
  `cargo xtask image --arch aarch64 --platform generic --profile release --output build/aarch64/login-cold-start-release.img`.
* Commit: `d1ad5ca8` (wave base).
* Image: `build/aarch64/login-cold-start-release.img`;
  SHA-256 `3f600506c5dca4ca71a7a76654ab029991828c304275b9996182eb41c2ff1298`.
* QEMU: `qemu-system-aarch64 -M virt,gic-version=2 -cpu cortex-a72 -m 512M -smp 1 -nographic`,
  AAVMF code/vars pflash, the image as a raw drive; TCG (aarch64 KVM is not available on this
  host).

The kernel banner in the preserved log is `RavenBSD kernel 0.1 (aarch64)`, and the loader is
`loader: RavenBSD loader 0.1 (aarch64)`; the image is the aarch64 release, not a Pi 4 board run.

## 2. Exact failing stage

The preserved serial log ends:

```
svcd: netd crash loop, giving up
svcd: getty up
svcd: getty timing: generation=1 start=8411 spawn=8730 first_poll=none deadline=none stalled=0 ready_at=8730
RavenBSD 0.1 (x86_64) (tty:console)
login: root
process: child exited (1)
init exited (1)
```

The harness matched `login: `, sent `root`, and waited for `Password: `. Instead of a password
prompt it saw `process: child exited (1)` and the supervisor then exited. Exactly:

**`login-exit-before-password`** — the getty/login child exits 1 after username input, before
the password prompt; the release session never reaches a shell.

The `RavenBSD 0.1 (x86_64)` console line is emitted by the getty/login session path (the kernel
banner itself is aarch64). It is recorded as observed, not as a platform conclusion.

The `pi4-release` generic image was also tried: it reaches `svcd: getty up` and the `rsh: ready`
console, but the Pi 4 platform services fail on QEMU virt (`devd` launches `platd`, which rejects
the synthetic resource plan) and no login prompt is produced. It is not a substitute.

## 3. Harness

`scripts/login-cold-start-aarch64.sh` is the K145 harness (the aarch64 counterpart of K143's
x86 table/summary shape):

* `--samples N` (default 5), `--smp N`, `--timeout SECONDS`, `--image FILE`, `--out DIR`,
  `--check`.
* It builds the release image only if missing, records commit/image SHA-256/host load, boots
  AAVMF/virt, sends `root` and `raven`, and measures `login:` → `Password:` and
  `Password:` → `raven# `.
* A login-stage failure prints `BLOCKED stage=...`, preserves the serial log under
  `build/login-cold-start-aarch64/sample-N.log`, and exits 3. It does not record a fake sample.
* On a working image it writes `samples.tsv` (`sample load1 accel smp commit login_ms
  password_ms`) and `summary.tsv` plus n/min/median/p90/max.

Acceptance attempt:

```
timeout 180 sh scripts/login-cold-start-aarch64.sh --samples 5
exit 3
login-cold-start-aarch64: BLOCKED stage=login-exit-before-password
login-cold-start-aarch64: BLOCKED at sample 1; preserved log: build/login-cold-start-aarch64/sample-1.log
```

Host load during the attempt was **28.70** (1-minute) and rose to **39.29**; this is a loaded
host, not an idle measurement. Because the failure is before any timing sample, no idle-host
claim is made either way.

## 4. What is missing / next step

The aarch64 dynamic release does **not** have a working getty/login session in QEMU virt at this
base. The exact missing piece is the login session path after username input; this is a
functional block, not a timing result. The K139/K135 loader/initfs work and the aarch64 login
binary/session need a working image before K145 can record 5 samples. Do not substitute a static
login, a Pi 4 board transcript, or a fake sample.

When the login stage works, run the same harness on an idle host:

```
timeout 180 sh scripts/login-cold-start-aarch64.sh --samples 5
```

and record the full load/hash/sample table. The after-K139 comparison is a later run; this ticket
does not claim it.

## 5. Donor reconciliation (DR-52)

This is an aarch64 boot/login measurement only. No donor code is used or copied; the harness
boots the first-party release image with QEMU/AAVMF and uses the existing console/getty path.
No C, no CDDL/GPL, no board sample.
