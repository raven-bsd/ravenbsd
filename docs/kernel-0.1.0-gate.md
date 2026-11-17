<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# Kernel 0.1.0 gate (S80)

Recorded gate at commit `44c34e0c` (`dave/S80`, the merge of `origin/master` `0ec8e619`, which
carries C65m + C47n). No product code changed; this is evidence only.

The previously open aarch64 item is **closed**: the Pi 4 release image builds again now that the
aarch64 libc links (C65m's per-arch map plus C47n's split long-double exports).

## 1. Gate table

| Command | Exit | Result |
|---|---|---|
| `cargo xtask image --arch aarch64 --platform pi4 --profile pi4-release` | 0 | **builds**: 335 544 320 bytes, sha256 `ee8e0a87f0864df70cf6bde37be524c1e3d0bbb1ccb53d8a802a025eca3b5614` |
| `sh scripts/pi4-release.sh --check` | 0 | `pi4-release: image check PASS (board gates remain UNVERIFIED)` |
| `sh scripts/kernel-gate.sh` | 0 | **PASS (27 cases)** |
| `cargo xtask qtest --case release-login --arch x86_64` | 0 | pass |
| `cargo xtask qtest --case trace-demo --arch x86_64` | 0 | pass, four-span tree |
| `cargo xtask qtest --case trace-demo --arch aarch64` | 0 | pass, four-span tree |
| `sh scripts/fault-injection.sh --profile release --arch x86_64 --soak 30m` | 0 | **30 runs, 30 pass, 0 fail** (not 24 h, see §3) |
| `make check-fast` | 0 | `check-fast: PASSED` |
| 24 h soak (`--soak 24h` = `86400`) | — | **NOT EXECUTED** |
| Pi 4 board serial/GENET | — | **UNVERIFIED** (no adapter, no board log) |

`kernel-gate.sh` rows (all pass): x86-64 `boot-uname`, `kernel-early-boot`, `exc-int3`, `exc-div0`,
`exc-pagefault`, `exc-doublefault`, `kernel-paging`, `kernel-timer`, `time-entropy`,
`time-entropy-cyclic`, `smp-user`, `smp-user-off`, `irq-dist`, `irq-dist-off`, `locking`,
`lockstat`, `ipc-donation`, `kmem-frames`, `kmem-frames-nopcpu`, `proc-death-cleanup`,
`trace-context`, `release-login`; aarch64 `boot-uname-aarch64`, `kernel-early-boot-aarch64`,
`kernel-exc-aarch64`, `kernel-paging-aarch64`, `kernel-timer-aarch64`.

## 2. aarch64 release image (closed)

```
$ cargo xtask image --arch aarch64 --platform pi4 --profile pi4-release
...
/media/bo/Rustworx/work/ravenbsd-minions/dave/ravenbsd/build/aarch64/ravenbsd-pi4.img
exit 0

$ sh scripts/pi4-release.sh --check
pi4-manifest: PASSED
pi4-release: initfs path /sbin/vfsd ok … /etc/pi4.dtb ok
pi4-release: /etc/pi4.dtb FDT ok totalsize=371 crc32=4047538b
pi4-release: image check PASS (board gates remain UNVERIFIED)
exit 0
```

## 3. Soak: 30 minutes, 30 runs (not 24 h)

```
fault-injection: run pass=1 fail=0 recovery_ms=...
...
fault-injection: run pass=29 fail=0 recovery_ms=49286
fault-injection: run pass=30 fail=0 recovery_ms=76676
fault-injection: soak done pass=30 fail=0 first_failure=none
```

`--soak 30m` is 1800 s; thirty complete `fault-restart-release` iterations, all passing, exit 0.
This is **not** the 24 h soak: the 24 h form is `--soak 24h` (or `86400`) and was **not executed**
in this round; no shorter run is reported as 24 h.

## 4. Q4 four-span trace

`trace-demo` passes on both architectures with the real `initfsd → blockd → fsd-ufs → vfsd` stack
and the mounted `file:/mnt/ten.txt` read:

```
vfsd: mounted / on initfs:
vfsd: mounted /mnt on ufs:
trace: spans=13 gaps=0 traced=11 untraced=2
trace: ... pid=0 kind=server op=1   # vfsd open
trace: ... pid=0 kind=server op=1   # fsd-ufs opens
trace: ... pid=0 kind=server op=2   # vfsd read
trace: ... pid=0 kind=server op=2   # fsd-ufs read
trace: ... pid=0 kind=io     op=7   # blockd block I/O
demo: ok
```

The qtest asserts every span line, so `client → vfsd → fsd-ufs → blockd` stays covered on x86_64
and aarch64.

## 5. Pi 4

UNVERIFIED. No `/dev/ttyUSB*` adapter and no board capture exist, so serial/GENET are not claimed.
Host/QEMU evidence is in `docs/pi4-qemu-gate.md` (A8c).
