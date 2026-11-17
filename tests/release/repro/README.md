<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# Release image reproducibility fixtures

This directory records the host-only two-build check for the 0.1.0 release images. The executable
check is `scripts/repro-release.sh`; the method, the commit-id input and the diagnosis table are
in `docs/releases/0.1.0-repro.md`; the recorded hashes for the branch commit are in
`tickets/dave/reports/C74cs.md`.

## Running it

```
sh scripts/repro-release.sh                 # x86_64 release and Pi 4 pi4-release, two builds each
sh scripts/repro-release.sh --arch x86_64
sh scripts/repro-release.sh --arch aarch64
sh scripts/repro-release.sh --self-test     # comparison logic only, no image build
```

The full run takes about one minute per architecture on a warm tree and needs
`RAVEN_RPI_FIRMWARE` (or `ravenbsd-resources/rpi-firmware/boot` in the usual workspace layout)
for the Pi 4 leg. A missing firmware directory is a hard failure, not a skip. The tree must be
clean: the image embeds `<git-id>[-dirty] <commit-date>`, so a dirty build is not a release
artifact. The script verifies the kernel carries the current clean commit id before accepting a
pair.

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

## Recorded run

The recorded pair for the commit that added this check (both images 335544320 bytes, build a =
build b, all constituent artifacts matching) is in `tickets/dave/reports/C74cs.md`. The hash is
commit-specific because the kernel carries the git id; do not compare it to a different commit.

## Scope

The two builds share `target/` and the checked-out tree, so the compiler is not re-run from an
empty cache. The image assembly (initfs, FAT, GPT, boot.toml) is regenerated on every build and
is covered; compiler-determinism isolation would be a separate, slower mode. No QEMU or board run
is part of this check.
