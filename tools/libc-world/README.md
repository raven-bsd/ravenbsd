# libc-world

<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

Build-the-world harness for ravenlibc (C72). It unpacks owner-supplied pristine tarballs, builds
them against the ravenlibc sysroot with a generated wrapper compiler, and records the evidence the
C33 census and the C74 dashboard consume. It has **no network code** and never runs target code.

## Quick start

```
cargo run -q -p libc-world -- check
cargo run -q -p libc-world -- list --subset smoke5
cargo run -q -p libc-world -- build --subset smoke5 --sysroot build/x86_64/sysroot
cargo run -q -p libc-world -- report
cargo run -q -p libc-world -- missing
cargo run -q -p libc-world -- gate --phase 1
```

`build` needs `build/<arch>/sysroot` from `cargo xtask libc-pack --arch <arch>` and the tarballs in
`ravenbsd-resources/world-src/` (the owner supplies them, H-11). Until the H-12 freeze every
`version`/`url`/`sha256`/`tarball` is the word `PLACEHOLDER`; a placeholder row is reported as
`placeholder`, never built. `check` enforces the placeholder discipline; `build` only enforces the
structure, so the documented real-subset procedure can fill a scratch copy of the file.

Exit codes: `0` a completed run even when packages fail; `1` `check` found an error or `gate`
refused; `2` usage, I/O or a missing required input; `77` a required host tool is absent
(`skipped: <tool> not found`).

## The list

`libc/ravenlibc/world.toml` is the 200-row list of doc 16 section 3.B.6.2: `n`, `name`, `tier`,
`build`, four placeholder fields, `needs`, `optional`, the verbatim `stresses` text, `configure`,
`flag_notes`, `make_args`, `jobs_make` and `tests`. The weighted total is 197.5 (rows 182, 183,
184, 188 and 195 are optional at 0.5). Subsets: `smoke5` = zlib lua sqlite pcre2 toybox, `quick4` =
zlib bzip2 lua dash, plus the computed `tier1` .. `tier5`. Build order is topological over `needs`,
ties by `(tier, n)`.

Build keys `ac`, `mk`, `cf`, `cm` and `kc` are implemented. `me`, `py`, `ca`, `go` and `b2` are
`skipped: builder-not-implemented:<key>` and never count as built (RD-new; the C++/Rust/Go rows
also need runtimes the scaffold does not have).

## What a run writes

Under `--out build/libc-world/<arch>/`:

* `bin/raven-cc` — the generated compiler wrapper (golden-tested; drives the link itself because
  clang links the unknown Raven triple through the host driver).
* `toolchain.cmake` — the CMake toolchain file.
* `<package>/src`, `build`, `dest`, `log/<step>.log` — work tree, kept only with `--keep`; the logs
  contain the step stdout and stderr.
* `calls.log` and `deps/*.d` under the log directory — the `-l` and system-header evidence.

Under `--results tools/libc-world/results/<arch>/`:

* `<package>.json` — the fixed-field package result. A rerun is byte-identical: no timestamps, no
  host paths, no thread-order dependence.
* `undef/<package>.undef` — `symbol<TAB>U|w`, sorted, exactly the C33 `--undef` input.
* `headers/<package>.txt` — sysroot-relative headers from the `-MD` files.
* `world.json`, `world.md`, `missing-symbols.tsv`, `headers.tsv`, `libs.tsv` — from `report`.

The census command printed by `missing` is:

```
cargo run -q -p libc-census -- run --undef tools/libc-world/results/<arch>/undef --resources <ravenbsd-resources>
```

## Patches and the gate

Every `patches/<package>/*.patch` starts with `Reason:` and `Class: libc|platform|toolchain`. A
`platform` patch whose reason contains a libc keyword (`missing function`, `missing header`,
`missing macro`, `errno`, `struct layout`, `sysconf`, `locale`, `iconv`, `symbol version`,
`undefined reference`, `implicit declaration`) is reclassified as a libc patch and the tool prints
`reclassified-as-libc: <file>`. C72 ships no patch for a real package.

`gate --phase 1 --min-built N` refuses when any `libc_patches > 0`, any
`configure_flags_disabled_because_libc` is non-empty, any `libc_probe_failed` belongs to a built
package, or fewer than `N` non-optional packages are built (default 100). Phase 2 requires every
non-optional row. Zero results are always `NOT MET`.

## Verification

```
cargo test -p libc-world          # unit tests plus the fixture end-to-end suite
cargo test -p libc-world --test e2e_fixtures -- --nocapture
RL_WORLD_REQUIRED=1 cargo test -p libc-world   # turns fixture SKIPs into failures
```

Tests that need `clang`, `llvm-ar`, `tar`, `patch`, `make`, `cmake` or `ninja` print
`SKIP: <tool> not found` when the tool is absent. The fixture packages under `tests/fixtures/pkgs`
are text; their tarballs are built at test time and the produced static libraries are never run.

## RD-new decisions recorded by C72

* Crate name `libc-world` (tool-directory convention), not `raven-libc-world`.
* Rows 186-200 are tier 4 except 196 (`libc-test`), which is tier 1.
* `smoke5` = zlib, lua, sqlite, pcre2, toybox.
* `--host-alias raven|linux` lets `configure` see a known triple without changing the compiler
  triple; the choice is recorded in `result.json`.
* `.undef` is delivered per package, not as one aggregate file.
* The outer `--jobs` runs ready packages in waves; each package still builds with its own
  `jobs_make`.
* `check` validates the freeze/placeholder discipline; `build` validates structure only, so a
  scratch copy with a few real rows can be used before the global freeze.
* `gate --results DIR` accepts either the arch directory or the results root; it auto-detects an
  `<arch>` child holding JSON files.
