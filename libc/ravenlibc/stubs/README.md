<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# Stub libraries

`stubs.toml` is the table of ticket C66 5.D (`17-crate-layout.md` 7.3, `14` 3.A.8 + delta
D-5). For every one of the 20 names the generator writes exactly two files:

| File | Bytes |
|---|---|
| `libNAME.a` | `!<arch>\n` (8 bytes: a valid archive with no members) |
| `libNAME.so` | `INPUT(libc.so.1)\n` (17 bytes: a linker script) |

No symbol, no `libNAME.so.N`, no `.pc` file. `libmd`, `libuuid` and `libc_heap_tiny` are not
stubs and must not appear in the output; `tests/features` holds the feature-macro acceptance
probes of 5.E.

## Commands

```
cargo xtask libc-stubs [--arch A] [--out DIR] [--check]
```

The default output is `build/<arch>/stubs/`. `--check` compares the directory byte for byte,
reports extra `lib*.a`/`lib*.so` files that the table does not name and exits 1 on a finding.
A missing output directory means "nothing generated yet" and is not an error; generate once
before checking in a clean tree. Bad flags are a usage error (exit 2).

## Why the files are empty

Configure scripts probe `epoll_create1`, `kqueue`, `pthread_create` by **linking**. A
present-but-failing function is the correct state; an absent symbol makes the wrong event loop
be selected at build time. Every name lives in `libc.so.1`, so the linker script adds no
`DT_NEEDED` beyond `libc.so.1` and the archive is accepted by GNU `ld` and `ld.lld` (C66 5.D
experiments).

## Autoconf

`autoconf-patches/` carries the RavenBSD `config.sub`/`config.guess` cases. Sending them to the
GNU `config-patches` list is an owner action.
