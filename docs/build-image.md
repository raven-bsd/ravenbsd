<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# Building release images from a clean tree (REL7)

Status: REL7 implementation. `cargo xtask image` and `cargo xtask qtest` now detect and build the
libc, brush and uutils prerequisites for the resolved profile and architecture. A release image no
longer depends on undocumented manual build steps. REL7b adds the profile-level release switch, so
`make image PROFILE=release` also builds its user programs with the release profile without an
environment variable.

## Prerequisite detection

The image and initfs paths call `xtask::initfs::ensure_prerequisites` before packing:

| Order | Prerequisite | Staged by | Outputs |
|---|---|---|---|
| 1 | libc pack | `cargo xtask libc-pack --arch <arch>` | `build/<arch>/libc.a`, `crt1.o`, `crti.o`, `crtn.o` |
| 2 | brush | `cargo xtask build brush --arch <arch>` | `build/<arch>/brush` |
| 3 | uutils slice | `cargo xtask build uutils --arch <arch>` | `build/<arch>/uutils/<name>` |

Rules:

* libc is built only when the resolved profile stages a libc consumer (brush or uutils), and only
  when an archive/crt output is missing. A base image with neither does not pay for it.
* brush is built only when the profile stages `build/brush` (legacy profile spelling) or
  `build/<arch>/brush`, and the arch-correct binary is missing.
* uutils is built only when the profile stages `build/<arch>/uutils/<name>` and a name is missing
  (K25c).
* Each step runs at most once per image/initfs invocation and in the order above, so brush and
  uutils always see the libc archive and crt objects they link against.
* A complete staging tree skips every build; normal incremental images stay fast.

The decision logic is host-tested in `xtask/src/initfs_ut.rs` (missing/partial/complete libc,
brush source mapping, uutils names, the combined plan). The build commands themselves are not run
inside the host tests.

## Release user programs (REL7b)

`profiles/release.toml` sets `[initfs] release = true`. The `release` field defaults to false and
merges additively across `extends` and `profiles/<name>.d/*.toml`: once a parent or fragment
enables it, the resolved profile keeps it. The profile parse and merge cases are host-tested in
`xtask/src/profiles_ut.rs`.

`xtask/src/initfs.rs` builds the resolved profile's user programs with the release profile when the
field is set. `make image PROFILE=release` is therefore self-contained. `RAVEN_USER_RELEASE`
remains an override: `=1` forces release and `=0` forces debug; any other value (or no value)
leaves the profile's decision. The build-mode decision is host-tested in
`xtask/src/initfs_ut.rs`. Rootfs profiles (`rootfs/<name>.toml`) keep their unconditional release
build in `xtask/src/image.rs`.

## Arch-correct staging

`cargo xtask build brush --arch <arch>` stages only `build/<arch>/brush`; it no longer writes a
mutable `build/brush`. The shell profile's legacy `src = "build/brush"` is resolved by the initfs
packer to `build/<arch>/brush`. Release profiles should use the explicit
`build/x86_64/brush` / `build/aarch64/brush` sources; there is no arch-independent brush path in a
release profile.

The QEMU (x86_64) path uses `build/x86_64/brush`; the Pi 4 path uses `build/aarch64/brush`.

## Commands

From a clean `build/` (or after removing only the selected prerequisites):

```sh
cargo xtask qtest --case shell-session --arch x86_64
cargo xtask image --arch x86_64 --profile shell-test
make image ARCH=x86_64 PROFILE=release
```

The qtest/image path prints the prerequisite actions it takes, for example:

```
==> libc x86_64: packing missing prerequisites (...)
==> brush x86_64: building missing staging binary (build/x86_64/brush)
==> uutils x86_64: building missing staging slice (...)
```

The `Makefile` targets remain thin wrappers:

```sh
make image ARCH=x86_64 PROFILE=shell-test
make brush ARCH=x86_64
```

The aarch64 Pi 4 prerequisites are the same tools with the other arch:

```sh
cargo xtask libc-pack --arch aarch64
cargo xtask build brush --arch aarch64
```

After that, the Pi 4 image path (`cargo xtask image --arch aarch64 --platform pi4 ...`) has its
libc and brush staging ready. The arch detection checks `build/aarch64/libc.a` and
`build/aarch64/brush` before packing.

## Acceptance evidence

The REL7 acceptance removes `build/x86_64/uutils`, `build/x86_64/brush` and `build/x86_64/libc.a`
and runs `cargo xtask qtest --case shell-session --arch x86_64`; the image path must rebuild all
three and the qtest must pass. The aarch64 commands above prove the Pi 4 prerequisites. The ticket
report carries the exact commands, exit codes and transcript. **OBSERVED** in
`tickets/kevin/reports/REL7.md`.

REL7b's acceptance removes `build/x86_64/user`, runs
`cargo xtask image --arch x86_64 --profile release` without `RAVEN_USER_RELEASE` and runs the
`release-login` qtest. The image must come from release user programs and the qtest must pass.
**OBSERVED** in `tickets/kevin/reports/REL7b.md`.

## Notes and limits

* The prerequisite detection is deliberately conservative: a missing `crt1.o`, `crti.o` or
  `crtn.o` re-runs the whole libc pack, because the pack is the supported producer of the group.
* The uutils slice is still the selected base-tools set, not the whole coreutils; K25a/K25c
  own that list.
* No network access is used; the uutils, brush and libc builds use the pinned toolchain and the
  vendored sources.
* The build artifacts live under `build/` and remain ignored; only the detection and staging code
  is committed.
