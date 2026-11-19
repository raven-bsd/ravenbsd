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
| 2 | dynamic link | `cargo xtask libc-link --arch <arch>` | `build/<arch>/libc.so.1`, `ld-elf.so.1`, `loader-manifest.toml` |
| 3 | brush | `cargo xtask build brush --arch <arch>` | `build/<arch>/brush` |
| 4 | uutils slice | `cargo xtask build uutils --arch <arch>` | `build/<arch>/uutils/<name>` |

Rules:

* libc is built only when the resolved profile stages a libc consumer (brush or uutils) or enables
  `dynamic`; a base image with neither does not pay for it.
* A dynamic profile also needs the `libc.so.1`/`ld-elf.so.1` pair and its manifest. An existing
  loader without a matching manifest is relinked, not reused.
* brush is built only when the profile stages `build/brush` (legacy profile spelling) or
  `build/<arch>/brush`, and the arch-correct binary is missing.
* uutils is built only when the profile stages `build/<arch>/uutils/<name>` and a name is missing
  (K25c).
* Each step runs at most once per image/initfs invocation and in the order above, so brush and
  uutils always see the libc archive and crt objects they link against.
* A complete staging tree skips every build; normal incremental images stay fast.

## Release loader freshness (K131)

A release profile is a production image, so its dynamic runtime must come from the current
sources. `ensure_prerequisites` removes `build/<arch>/libc.so.1`, `build/<arch>/ld-elf.so.1` and
`build/<arch>/loader-manifest.toml` together with the brush/uutils staging and target caches before
it plans the build. `libc-link` therefore always runs and produces a fresh loader.

After linking, `libc-link` always runs the loader side of the K7 ELF gate and fails the build on a
mismatch:

* the first `PT_LOAD` virtual address is the supported image base `0x1000`;
* the SONAME is `ld-elf.so.1` and the entry point is `_rtld_start`;
* the loader exports exactly the 17 `libdl`/rtld names, has no `PT_INTERP`, `PT_TLS` or `DT_NEEDED`,
  and has no non-relative relocation.

It also writes `build/<arch>/loader-manifest.toml`:

```text
arch = "x86_64"
revision = "d48b0eac"
image_base = "0x1000"
interpreter = "/libexec/ld-elf.so.1"
size = 107128
crc32 = "08c0f51a"
sha256 = "7bc53ceceaf31bdc878d31461075ccd3f0a1029028003af1a82f30d3cba46e09"
```

`initfs::build` parses the finished archive before writing the image and fails when
`/libexec/ld-elf.so.1` is absent, differs byte-for-byte from the fresh loader, or when the
manifest's base, interpreter or CRC-32 does not match. The image can therefore not silently carry
a stale loader. The regression test plants a 14-byte `STALE-NOT-ELF` loader and requires the
release initfs build to replace it:

```sh
sh tests/release/check-loader.sh
```

**OBSERVED** in `tickets/kevin/reports/K131.md`.

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

## Release kernel and loader (K170)

A release image means **release kernel + release loader + release user programs**. Before K170 the
image path always built the kernel and UEFI loader with `release = false`; `[initfs] release` only
covered the user programs.

The image build mode now comes from the resolved profile:

* The release image names (`release` on x86_64, `pi4-release` on aarch64) are release by default.
* Any profile can opt in with:

  ```toml
  [image]
  release = true
  ```

  The field is additive across `extends` and `profiles/<name>.d/*.toml`, like
  `[initfs] release`. This is the explicit switch for demo and test profiles (for example a
  `demo-login` profile in the demo workspace); debug and test profiles keep debug builds.

Both the kernel and loader are built with `--release` and the DR-22 LTO settings. Artifacts are
profile-qualified and stamped:

| Requested | Artifact | Stamp |
|---|---|---|
| release kernel | `build/<arch>/kernel.elf` | `kernel.elf.profile` = `release` |
| release loader | `build/<arch>/loader.efi` | `loader.efi.profile` = `release` |
| debug kernel | `build/<arch>/kernel-debug.elf` | `kernel-debug.elf.profile` = `debug` |
| debug loader | `build/<arch>/loader-debug.efi` | `loader-debug.elf.profile` = `debug` |

A release build refreshes the historical `kernel.elf`/`loader.efi` names. A debug build writes only
the `-debug` names when a release artifact already occupies the historical path, so `make release`
followed by a debug `make image` does not invalidate the release kernel or loader. The image step
verifies the stamp of the artifact it actually embeds; a release image whose kernel or loader stamp
says `debug` (or is missing) is rejected before the image is written. Release kernels also keep the
clean-HEAD build-id check.

The Pi 4 image path builds the Pi kernel with `RAVEN_PI4=1` in the requested profile and passes that
ELF to the Pi layout; the Pi boot partition has no UEFI loader.

Host tests cover the profile decision (`[image] release`, additive merge), the profile-qualified
names and the stamp verify/reject path in `xtask/src/{profiles,build,image}_ut.rs`.

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
