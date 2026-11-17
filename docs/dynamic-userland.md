<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# Dynamic userland - build recipe and remaining integration

Date: 2026-11-17.  Base: `origin/master` `33fd9653`.

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

## Remaining integration (not landed)

1. `xtask/src/build.rs`: a dynamic-userland mode for `Component::User` (x86_64) that uses the
   wrapper and flags above instead of `relocation-model=static` plus
   `userland/linker/x86_64.ld`.
2. `xtask/src/initfs.rs`: when the profile requests dynamic userland, require
   `build/<arch>/{libc.so.1,ld-elf.so.1}` and run `cargo xtask libc-link`.
3. Profiles `shell`/`release`: stage `/lib/libc.so.1` and `/libexec/ld-elf.so.1`, set the dynamic
   switch, and verify boot; `init` may remain static only with a named reason if the kernel cannot
   load its interpreter from initfs before the file service.
4. `tests/qtest/cases/dynamic-userland.toml` + fixture: readelf `PT_INTERP`/`DT_NEEDED` on a
   packaged program and boot the shell/release image to prove the loader reaches `main`.

The base still has the K26h non-brush tool-runtime hang (`date`/`uname`/`mkdir`) which must be
fixed before `base-tools`/`shell-session` can be used as dynamic-userland acceptance.
