<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# Pi 4 aarch64 QEMU boot gate (K096b)

Date: 2026-11-21
Tree: `b04e71ad87d0273ab9d86367b912a30c300c69ac`

Scope: host-side aarch64 release-image boot evidence. **No board, HDMI monitor or USB-keyboard
claim**; those remain UI1 hardware gates.

## Image

```sh
timeout 1200 cargo xtask image --arch aarch64 --profile release \
  --output build/qtest/k096b-aarch64.img
```

- Exit 0, measured **423 s**.
- Size **335,544,320 bytes** (320 MiB).
- SHA-256 `c6c2527ff8b4a20b43f392776d5e480bd8115afa710a2bf063afdca69383b2a2`.
- The build log is `tests/evidence/pi4/<sha>/k096b-image-build.log`.

## Boot result

```sh
cp build/qtest/k096b-aarch64.img build/qtest/k096b-aarch64-login.img
timeout 600 cargo xtask qtest \
  --case tests/evidence/pi4/k096b/aarch64-login.toml \
  --arch aarch64 --no-build
```

- **`k096b-aarch64-login: pass`**, 24.1 s, all expectations matched, no forbidden lines.
- Serial (`tests/evidence/pi4/<sha>/k096b-serial.log`) contains:

```text
loader: RavenBSD loader 0.1 (aarch64)
RavenBSD kernel 0.1 (aarch64) b04e71ad 2026-11-21
R A V E N   B S D
Welcome to RavenBSD 0.1 aarch64
login: root
Password:
raven#
init exited (0)
```

The styled screen, login/password, root prompt and clean `init exited (0)` are the host-side
contract. The QEMU boot report also shows `10 up, 3 failed` (pcid/netd/devd) because this is not
the Pi 4 board; the login path still completes and is the only claim here.

## Test-case finding (for the qtest owner)

- `tests/qtest/cases/release-login.toml` is **x86_64-only** (`arch = "x86_64"`, x86_64 loader/kernel
  literals, `console=ttyS0`), so `--arch aarch64` cannot use it as the ticket text suggests.
- `tests/qtest/cases/login-aarch64.toml` is the real aarch64 case, but its expectation
  `^RavenBSD 0\.1 \(aarch64\) \(tty:console\)$` no longer appears on current master; the case
  times out after a successful login/prompt/exit. That failure is preserved as
  `k096b-login-aarch64-stale.log/json`.
- K096b therefore adds the evidence case `tests/evidence/pi4/k096b/aarch64-login.toml` with the
  current markers (styled screen, welcome, login/password, `raven#`, clean exit). It does not
  weaken or delete the existing cases; their stale expectation is a separate test-owner fix.

## K411 five-boot stability sample

Tree `c685aa6ce9e20eed536309111e2a63463c56d45a`, image sha256
`1539663743cda9341a232a474206db9376de82619200b9a61f47834241f26d2f` (320 MiB, build 244.89 s).
Five consecutive `login-aarch64` boots, `--no-build`, KVM host QEMU:

| Run | Exit | Wall s | Host load1 | Panic/TLB | Kernel |
|---:|---:|---:|---:|---:|---|
| 1 | 0 | 65 | 27.07 | 0 | `c685aa6ce` |
| 2 | 0 | 30 | 44.27 | 0 | `c685aa6ce` |
| 3 | 0 | 44 | 35.85 | 0 | `c685aa6ce` |
| 4 | 0 | 44 | 35.46 | 0 | `c685aa6ce` |
| 5 | 0 | 26 | 31.43 | 0 | `c685aa6ce` |

p50 44 s, p90 65 s. Every boot reached the styled screen, `login:`/`Password:`/`raven# ` and
`init exited (0)`; no panic, exception or TLB line. Host QEMU only - no board/HDMI claim.
Artifacts: `tests/evidence/pi4/c685aa6ce9e20eed536309111e2a63463c56d45a/`.

## Hardware gates

| Gate | Status |
|---|---|
| aarch64 release boot to login in QEMU | **PASS**, host-only |
| Pi 4 SD/UFS serial boot | **HARDWARE** (K383 owner checklist) |
| HDMI monitor + USB keyboard login | **HARDWARE** (UI1) |
| E-Ink panel photo/refresh | **HARDWARE** (E5) |

## Commands and caps

| Command | Cap | Exit | Result |
|---|---:|---:|---|
| `cargo xtask image --arch aarch64 --profile release --output build/qtest/k096b-aarch64.img` | 1200 s | 0 | 423 s, image built |
| `cargo xtask qtest --case tests/evidence/pi4/k096b/aarch64-login.toml --arch aarch64 --no-build` | 600 s | 0 | pass, 24.1 s |
| `cargo xtask qtest --case login-aarch64 --arch aarch64 --no-build` | 600 s | 1 | stale banner expectation; boot itself succeeds |
| `make check-fast` | 900 s | 0 | `check-fast: PASSED` |

## Evidence files

Under `tests/evidence/pi4/b04e71ad87d0273ab9d86367b912a30c300c69ac/`: image build log, image
SHA-256/size, passing serial and JSON, stale-case serial/JSON, and `SHA256SUMS`.

The earlier `d8f32734` Pi 4 host/QEMU sweep (pi4-root/devd/usb/emmc2/platd and the board-only
UNAVAILABLE table) is preserved at `b04e71ad:docs/pi4-qemu-gate.md` in git history; this page is the
current K096b aarch64 boot record.
