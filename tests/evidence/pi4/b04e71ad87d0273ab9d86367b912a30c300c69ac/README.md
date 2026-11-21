<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# K096b aarch64 QEMU boot evidence

Tree `b04e71ad`. Host-side aarch64 release boot, no board/HDMI claim.

| File | Content |
|---|---|
| `k096b-image-build.log` | `cargo xtask image --arch aarch64 --profile release`; 423 s, 320 MiB |
| `k096b-image.sha256` | image SHA-256 + size/mtime |
| `k096b-serial.log` | passing QEMU boot serial: loader, kernel, styled screen, login, `raven#`, clean exit |
| `k096b-qtest.json` | case result `pass`, 24.1 s, all expectations matched |
| `k096b-login-aarch64-stale.log/json` | existing `login-aarch64` case times out on its stale `RavenBSD 0.1 (aarch64) (tty:console)` expectation |
| `SHA256SUMS` | hashes of these records |

The K096b evidence case is `tests/evidence/pi4/k096b/aarch64-login.toml`; the platform hardware
gates remain HARDWARE.
