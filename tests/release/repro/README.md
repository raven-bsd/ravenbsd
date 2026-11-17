<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# Release image reproducibility fixtures

This directory records the host-only two-build check for the 0.1.0 release images. The executable
check is `scripts/repro-release.sh`; the recorded hashes and the determinism verdict are in
`docs/releases/0.1.0-repro.md`.

## Running it

```
sh scripts/repro-release.sh                 # x86_64 release and Pi 4 pi4-release, two builds each
sh scripts/repro-release.sh --arch x86_64
sh scripts/repro-release.sh --arch aarch64
sh scripts/repro-release.sh --self-test     # comparison logic only, no image build
```

The full run takes about one minute per architecture on a warm tree and needs
`RAVEN_RPI_FIRMWARE` (or `ravenbsd-resources/rpi-firmware/boot` in the usual workspace layout)
for the Pi 4 leg. A missing firmware directory is a hard failure, not a skip.

## What the self-test covers

`--self-test` creates synthetic files in a temporary directory and checks the three cases the
comparison code must classify:

| Case | Fixture | Expected |
|---|---|---|
| identical | two copies of the same bytes | `identical` |
| same size, different content | `abcdef` vs `abcdeg` | `content`, first differing byte 6 |
| different size | `abc` vs `abcd` | `size` |

It also checks that SHA-256 returns 64 hex characters. It never touches `build/` and never claims
an image verdict.

## Recorded run (2026-11-17, `24f8cbff`)

| Image | Size | SHA-256 (a = b) | Verdict |
|---|---|---|---|
| x86_64 `release` | 335544320 | `f270369a8664b7aa0d1b87e3fd295291d3ee9f0ec051cb2011c8e4350ae8df14` | REPRODUCIBLE |
| Pi 4 `pi4-release` | 335544320 | `230d9a0ce243cd55a5144d0c2ecd5c6836639994845d2996cc08bd9e23bef73c` | REPRODUCIBLE |

The constituent `kernel.elf`, `loader.efi` and `initfs.img` hashes matched between the two builds
for both images. The complete artifact hashes, environment and the future-difference diagnosis
table are in `docs/releases/0.1.0-repro.md`.

## Scope

The two builds share `target/` and the checked-out tree, so the compiler is not re-run from an
empty cache. The image assembly (initfs, FAT, GPT, boot.toml) is regenerated on every build and
is covered; compiler-determinism isolation would be a separate, slower mode. No QEMU or board run
is part of this check.
