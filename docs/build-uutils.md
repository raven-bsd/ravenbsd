<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# Building the uutils slice

K25a moved the first coreutils set (`ls`, `cat`, `cp`, ...) to the uutils workspace under
`userland/uutils`, built for the Raven std target and staged at `build/<arch>/uutils/<name>`. The
`shell`/release profiles install those staged files at `/bin/*` with `generated = true`.

## Manual build

```
cargo xtask build uutils --arch x86_64
cargo xtask build uutils --arch aarch64
```

The command runs the pinned `+ravenbsd` toolchain against
`userland/uutils/Cargo.toml`, selects the 30 K25a utilities, links them with the staged
`libc.a`/`crt*.o` and the uutils linker script, and copies each binary to
`build/<arch>/uutils/`. It defaults to release because the 64 KiB user stack cannot hold a debug
`ls`/`cat` frame. Before the first build, create the libc/crt staging with:

```
cargo xtask libc-pack --arch x86_64
```

## Automatic build from an image/qtest

`cargo xtask image`, `make image`, `cargo xtask initfs`, `cargo xtask qtest` and the Pi 4 image
path now inspect the **resolved** profile, including `extends` and sorted `*.d` fragments. If any
`initfs.files` source has the exact shape

```
build/<arch>/uutils/<name>
```

and the staged file is missing, the image path runs the uutils slice build once before packing.
The check is repeated after the build; a binary that is still absent aborts the image with the
missing names. If every referenced staging file exists, nothing is rebuilt.

The detection and build are in `xtask/src/initfs.rs::ensure_uutils` and are called by both the
normal initfs path and the release/rootfs initfs path in `xtask/src/image.rs`. The profile parser
still treats `generated = true` sources as build-time artifacts and does not require them to exist
at load time.

## Clean-tree check

```
rm -rf build/x86_64/uutils
cargo xtask qtest --case shell-session --arch x86_64
```

The qtest must build the slice itself, stage `/bin/ls`, `/bin/cat`, ... and pass. The release image
is then reproducible from a clean `build/<arch>/uutils` without a separate manual step.

## Limits

* The slice contents, profile paths and `userland/uutils/**` are unchanged by K25c.
* The build still needs the libc/crt staging (`cargo xtask libc-pack`) and the pinned
  `+ravenbsd` toolchain; those are setup prerequisites, not profile inputs.
* `generated` files outside `build/<arch>/uutils/` are not auto-built by this rule.
