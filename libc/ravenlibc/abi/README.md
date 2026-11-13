<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# ravenlibc ABI data

One source of truth for the external ABI of doc 14 part A. `raven-libc-hdrgen` reads these
TOML files and writes both views: the Rust crate `crates/raven-libc-abi` and the constant
parts of the C headers under `generated/include/`. Nothing here is edited by hand after
generation; a change is a TOML diff plus the generated diff in the same commit.

| File | Content |
|---|---|
| `types.toml` | scalar C types, widths, signedness, per-arch `wchar_t`/`char` |
| `structs.toml` | `[[record]]` field lists and `[[opaque]]` sized blobs |
| `deviations.toml` | the complete, reasoned difference register (musl and `raven-linux-abi`) |
| the 14 group files | object-like constants: `errno signal fcntl mman stat socket netinet poll time resource limits sysconf pthread misc` |

## Commands

```sh
cargo run -q -p raven-libc-hdrgen -- check       # committed tree must equal regeneration
cargo run -q -p raven-libc-hdrgen -- generate    # rewrite the generated tree
cargo run -q -p raven-libc-hdrgen -- verify --target all
cargo run -q -p raven-libc-hdrgen -- provenance --date $(date +%F)
```

`check` validates every table (L1 layout arithmetic, alias resolution, types, uniqueness,
`omit` reasons) and fails on a stale, missing or extra generated file. `verify` reproduces the
doc 14 3.A.3 musl measurement with clang, compiles the local `_Static_assert` probe (L2),
probes every musl-derived constant (H1/H2/H3), checks the compiler identity macros and
compiles every generated header for the three targets. `provenance` prints the `[[import]]`
entries for the root `PROVENANCE.toml`.

## Field-name conventions

* `musl_field = "__tm_gmtoff"` maps a public RavenBSD field name to musl's private spelling
  for the layout probe.
* `musl_field = ""` marks a named padding field (`__pad`, `__pad1`, ...) that musl expresses
  only as implicit padding; the probe skips it.
* Records with `deviates` (only `stat`) are compared to musl by size/align; their field list
  is the frozen RavenBSD layout, not musl's arch-specific one.

## Licence

`types.toml`, `structs.toml`, `deviations.toml` and this file are first-party (BSD-3-Clause).
The 14 group files are transcribed from musl (MIT, `musl/COPYRIGHT`; the notice block is in
each file) and their generated Rust and C outputs carry the same notice. `generated/include/arch/<a>/bits/syscall.h`
comes from `raven-linux-abi` (`linux-raw-sys`, MIT). `abi/census/` belongs to C33 and is not
touched here.
