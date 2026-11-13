<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# libc-census

Host-only, offline census for ravenlibc. It reads the donor checkouts and the
owner-provided ports snapshots, counts names, computes rings, and writes the
committed data under `libc/ravenlibc/abi/census/`. Outputs are facts only:
names, tags, headers, counts and file existence. Donor prose and donor code are
never copied into an output.

The tool is first-party BSD-3-Clause. Donor facts used: OpenBSD and FreeBSD
symbol lists (licence notices absent: names only), bionic `libc.map.txt` (data),
the glibc manual (LGPL: name/tag/header facts only), and the illumos
`mapfile-vers` (CDDL-1.0: names and counts only, parsed with the ticket grammar
and never printed). musl is scanned as source text only.

## Commands

```
cargo run -q -p libc-census -- run [--resources DIR] [--donors a,b,..]
    [--aports DIR] [--pkgsrc DIR] [--freebsd-ports DIR] [--openbsd-ports DIR]
    [--sources DIR] [--undef DIR] [--names FILE] [--out DIR] [--jobs N]
    [--no-prefilter] [--rev TEXT]
cargo run -q -p libc-census -- merge [--out DIR] [--census DIR] [--assignments FILE]
cargo run -q -p libc-census -- missing --implemented FILE [--out DIR]
    [--min-packages 3] [--fail-p0]
cargo run -q -p libc-census -- compare-table --table FILE [--resources DIR] [--require-all]
cargo run -q -p libc-census -- seed-assignments --api-surface FILE [--out FILE]
```

Defaults: `--resources` is the nearest `ravenbsd-resources` at or above the
current directory, else `../ravenbsd-resources`. `--donors` is all nine
(`glibc,musl,freebsd,netbsd,openbsd,bionic,illumos,mlibc,picolibc`). `--out` is
`tools/libc-census/out` (git-ignored). `--jobs` is `min(8, available
parallelism)`. `--rev` is `$LIBC_CENSUS_REV`, else the text `unversioned`; the
tool never runs `git`. `merge` defaults to `libc/ravenlibc/abi/census`.

Exit codes: 0 success, 1 a gate failed (`compare-table`, `missing --fail-p0`),
2 usage or I/O error. `run` exits 0 after a completed run even when inputs are
missing; the gaps are recorded in `run-summary.json`.

`run` writes `cells.json`, `symlists.json`, `standards.json`, `ports.json`,
`uses.json` and `run-summary.json`. `merge` writes `symbols.toml`, `census.json`,
`census.md`, `ring_diff.md`, `patchset-gaps.md` and, only when absent,
`known-differences.toml` (reviewer-owned afterwards). `missing` reads
`<out>/symbols.toml` when present, else the committed census file.

## Snapshot layouts

| Flag | Package id | Patch files |
|---|---|---|
| `--aports DIR` | `<repo>/<pkg>/APKBUILD` -> `<pkg>` | `*.patch`, `*.diff` |
| `--pkgsrc DIR` | `<category>/<pkg>/Makefile` -> `<category>/<pkg>` | `<pkg>/patches/patch-*` |
| `--freebsd-ports DIR` | `<category>/<port>/Makefile` -> `<category>/<port>` | `<port>/files/patch-*` |
| `--openbsd-ports DIR` | `<category>/<port>/Makefile` -> `<category>/<port>` | `<port>/patches/patch-*` |

`--sources` expects one directory per package and scans C/C++ sources for
angle-bracket includes (`S1`) and textual call sites, and build files for
`-lNAME`, `AC_CHECK_LIB` and `AC_SEARCH_LIBS` (`S3`). `--undef` expects
`<pkg>.undef` files with `symbol<TAB>U|w` lines (`S2`). A missing snapshot is
`skipped: ...`, never an error.

## Census rules

Doc 15 3.B.2-3.B.3 is reproduced mechanically. Per donor the cell is `D/H`:
`D` counts matching non-header files and `H` matching `.h`/`.hp`/`.hpp` files.
Paths containing `test`, `tests`, `regress` or `testsuite` are excluded, hidden
entries are skipped, files over 16 MiB and files with a NUL byte in the first
8 KiB are skipped and counted. The `n6` count covers glibc, musl, FreeBSD,
OpenBSD, illumos and bionic; NetBSD, mlibc and picolibc are shown but not
counted. Rings are doc 02 section 1: a standard tag is ring 0, two or more
families ring 1, one family ring 2 with that personality, no family ring 3.
`assignments.toml` may override a ring only with a `ring_override` reason; the
mechanical value stays in `ring_computed` and differences appear in
`ring_diff.md`.

## Fixtures

`tests/fixtures/` is a small invented corpus: nine donor trees, the five symbol
list grammars, four ports snapshots, two source packages, undefined-symbol
lists, a six-row doc 15 excerpt and a doc 02 excerpt. `tests/fixtures/expected/`
holds the golden merge outputs. Nothing in the fixtures is copied from a donor.
