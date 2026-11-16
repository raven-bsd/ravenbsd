<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# aarch64 vfsd and the fsd-ufs fixture

K54a fixed the aarch64 `file:` read `EBADF` by making vfsd own every open description instead of
returning `Opened::Attach` from the target server. K54b makes the literal K54 qtest work on both
architectures.

## Fixture selection

The K54 `fsd-ufs-test` profile stages its probe from `build/fixtures/k54-ufs/probe`. The original
`tests/fixtures/k54-ufs/build.sh` always built `x86_64-unknown-none`, so
`cargo xtask qtest --case fsd-ufs --arch aarch64` booted an aarch64 guest with an x86-64 probe and
failed before vfsd with `bad ELF: BadMachine`.

`tests/fixtures/fsd-ufs/build.sh` keeps the same output path but selects the probe target and
linker from `RAVEN_QT_ARCH`:

| `RAVEN_QT_ARCH` | target | linker |
|---|---|---|
| `x86_64` | `x86_64-unknown-none` | `userland/linker/x86_64.ld` |
| `aarch64` | `aarch64-unknown-none-softfloat` | `userland/linker/aarch64.ld` |

`tests/qtest/cases/fsd-ufs.toml` now runs this script as its `pre` step. The same QEMU case then
runs the UFS mount and `file:` reads through vfsd on either architecture.

## Evidence

```
cargo xtask qtest --case fsd-ufs --arch aarch64   # pass
cargo xtask qtest --case fsd-ufs --arch x86_64    # pass
cargo xtask qtest --case vfs-aarch64 --arch aarch64  # pass
```

The guest prints the mount, `ten.txt`, `one.txt`, the directory listing, the symlink target, the
error checks, `umount ok` and `init exited (0)`.
