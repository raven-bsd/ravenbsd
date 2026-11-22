<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# K411: aarch64 release boot 5x

Host QEMU aarch64 release-image boots at tree `c685aa6ce9e20eed536309111e2a63463c56d45a`.
No board, HDMI monitor or USB-keyboard claim; host QEMU only.

## Image

- `cargo xtask image --arch aarch64 --profile release --output build/qtest/login-aarch64.img`
- build 244.89 s, exit 0
- size 335,544,320 bytes
- sha256 `1539663743cda9341a232a474206db9376de82619200b9a61f47834241f26d2f`
- log: `image-build.log`

## Boots

`cargo xtask qtest --case login-aarch64 --arch aarch64 --no-build`, five consecutive runs.

| Run | Exit | Wall s | Host load1 | Panic/TLB lines | Kernel banner | Result |
|---:|---:|---:|---:|---:|---|---|
| 1 | 0 | 65 | 27.07 | 0 | `c685aa6ce` | pass |
| 2 | 0 | 30 | 44.27 | 0 | `c685aa6ce` | pass |
| 3 | 0 | 44 | 35.85 | 0 | `c685aa6ce` | pass |
| 4 | 0 | 44 | 35.46 | 0 | `c685aa6ce` | pass |
| 5 | 0 | 26 | 31.43 | 0 | `c685aa6ce` | pass |

p50 44 s, p90 65 s, max 65 s. Every serial contains the styled screen
(`R A V E N   B S D`, `Welcome to RavenBSD 0.1 aarch64`), `login:`/`Password:`/`raven# ` and
`init exited (0)`. No `panic`, `EXCEPTION`, `tlb shootdown`, `bad ELF` or `getty failed` line.
