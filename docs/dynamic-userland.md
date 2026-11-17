<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# Dynamic userland - build recipe and remaining integration

Date: 2026-11-17.  Base: `origin/master` `1939ae69`.

## What works

`libc-link` produces `build/x86_64/libc.so.1` and `build/x86_64/ld-elf.so.1` and stages
`build/x86_64/sysroot/{lib/libc.so.1,libexec/ld-elf.so.1,usr/lib/libc.so}`.

A Rust user program links as a normal dynamic PIE with:

```
-C linker=userland/linker/rust-lld-dynamic.sh
-C code-model=small -C relocation-model=pic -C force-frame-pointers=yes
-C link-arg=-pie -C link-arg=-Bdynamic
-C link-arg=-dynamic-linker -C link-arg=/libexec/ld-elf.so.1
-C link-arg=--image-base=0x400000
-C link-arg=-z -C link-arg=max-page-size=4096
-C link-arg=-z -C link-arg=common-page-size=4096
-C link-arg=-Lbuild/x86_64/sysroot/lib
-C link-arg=-Lbuild/x86_64/sysroot/usr/lib
-C link-arg=--allow-shlib-undefined -C link-arg=--no-as-needed -C link-arg=-lc
```

The wrapper strips the built-in target's `-static`/`-static-pie`/`--no-pie`; `-Bdynamic` alone
cannot undo `-static`. Do not pass the static `userland/linker/x86_64.ld`: with a custom PHDRS
script LLD omits `PT_INTERP`/`PT_DYNAMIC`. The default LLD layout is required.

## Observed shape

Built `raven-uname` with the recipe above:

```
$ file .../raven-uname
ELF 64-bit LSB pie executable, x86-64, dynamically linked,
interpreter /libexec/ld-elf.so.1, stripped

$ readelf -l .../raven-uname | grep -E 'INTERP|DYNAMIC'
  INTERP  0x0000000000000270 ... 0x0000000000400270
  DYNAMIC 0x00000000000029b8 ... 0x00000000004049b8

$ readelf -d .../raven-uname | grep NEEDED
 0x...0001 (NEEDED) Shared library: [libc.so.1]
```

The existing `tests/fixtures/dynamic-exec` C binary shows the same `PT_INTERP` and
`DT_NEEDED libc.so.1` with the established C recipe.

## Integration landed (K97 rework on 1939ae69)

The switch is on: `profiles/shell.toml` and `profiles/release.toml` set
`[initfs] dynamic = true`. `xtask/src/initfs.rs` stages `build/<arch>/libc.so.1` at
`/lib/libc.so.1` and `build/<arch>/ld-elf.so.1` at `/libexec/ld-elf.so.1` for every dynamic
profile, and requires `cargo xtask libc-link` when either is missing. The Rust dynamic link mode
stays x86_64-only (`build::dynamic_supported`); other arches remain static and stage nothing.

`init` and `/sbin` stay static: the kernel enters `/init` before the file service, so the early
bootstrap cannot resolve `/libexec/ld-elf.so.1` from the initfs. `/bin` programs are the dynamic
userland.

`tests/qtest/cases/dynamic-userland.toml` runs
`tests/fixtures/dynamic-userland/check.sh` first. The pre script builds the `shell-test` initfs
and asserts `raven-rsh`/`raven-uname` carry `PT_INTERP /libexec/ld-elf.so.1` and
`DT_NEEDED libc.so.1`, `/init` and `/sbin/initfsd` stay static, and both runtime files exist.
The boot then requires `rsh: ready`, i.e. the dynamic `/bin/rsh` reached `main`.

## Current boot block (C74o)

The image builds and the readelf/staging checks pass, but the boot still stops before `main`:

```
svcd: all services up
ld-elf.so.1: relocation failed
process: child exited (127)
init exited (127)
```

C74l traced this to the strong undefined `main` in `libc.so.1` (a `GLOB_DAT` the loader cannot
resolve), not to the RELATIVE path; the C `rtld-relative` control passes. C74o owns the libc
start-ABI fix. Until it merges, `dynamic-userland` and every dynamic image test (`release-login`,
`base-tools`, `shell-session`) stop at the same line. The `check.sh` evidence above is the
build-side proof that the switch, staging and `PT_INTERP`/`DT_NEEDED` are correct.

The K26h/C74p non-brush tool-runtime hang (`date`/`uname`/`mkdir`) is still open; with dynamic
on, the loader block is reached first, so that hang is not re-measured here.

