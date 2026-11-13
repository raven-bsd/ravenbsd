<!--
SPDX-License-Identifier: BSD-3-Clause
Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
-->

# libc-oracle

Host tool that builds donor C files into shared objects whose symbols are renamed `oracle_*`, and
that vendors donor test C unchanged. It is the build half of the L2 differential harness
(`tests/libc/oracle`, crate `raven-libc-oracle`); ticket C35, `architecture/raven-libc/05-transpile-guide.md`
sections 6.1-6.7 and `donor-map/16-size-testing-oracles.md` 3.B.2-3.B.5.

The tool compiles C at **test time** from the read-only donor checkout. No donor `.c` is committed,
no `build.rs` compiles C, and no Cargo dependency (`cc`, `bindgen`, `libloading`) is used. Every
process is spawned through `std::process::Command` with an argument vector, never a shell string.

## Commands

```
libc-oracle build  (<family> | --all) [--opt O0,O2] [--sanitize] [--out DIR]
                   [--recipes DIR] [--resources DIR] [--no-pin-check]
libc-oracle adhoc  <donor> <file> [--opt O2] [--out DIR] [--recipes DIR]
                   [--resources DIR] [--no-pin-check]
libc-oracle list   [--recipes DIR]
libc-oracle check  [--recipes DIR] [--resources DIR]
libc-oracle vendor --donor D --from DIR --to DIR [--data] [--dry-run]
                   [--date YYYY-MM-DD] [--resources DIR] [--no-pin-check]
```

Defaults: `--recipes tests/libc/oracle/recipes`, `--out build/libc-oracle`; `--resources` is the
nearest `ravenbsd-resources` directory at or above the current directory.

Exit codes: `0` success, `1` a gate refusal (licence, donor pin, export mismatch), `2` usage, I/O or
recipe error, `77` a required tool (`cc`, `ld`, `nm`, `objcopy`) or `git` is missing. The `77` line
is always `skipped: <tool> not found` so a caller can report the run as skipped. Non-Linux hosts
print `skipped: unsupported host` and exit `77`.

## What a build produces

For every optimisation level, in a clean `build/libc-oracle/<family>/<opt>/`:

1. Each source is compiled with `cc -fno-builtin -fPIC -w -std=gnu11 -O<opt>`, the `defaults.toml`
   defines, the recipe defines, include directories and whitelisted flags. `--sanitize` adds
   `-fsanitize=undefined -fno-sanitize-recover=undefined` to the compile and the link.
2. `ld -r` merges the objects; `nm -g --defined-only` is parsed in Rust and the sorted name set must
   equal the recipe's `exports`.
3. `objcopy --redefine-syms` renames **defined globals only** (`name oracle_name`), so external
   references such as `memcpy` stay bound to the host libc. `cc -shared` links
   `liboracle_<family>.so`; `nm -D --defined-only` must contain exactly `oracle_<export>`.
4. `exports.txt` (sorted `oracle_*` names) and `build.json` (`family`, `opt`, `donors`, `commands`,
   `sources`) are written without timestamps or absolute home paths. Two builds of the same input
   are byte-identical.

The `nm -D` listing is printed, then the last stdout line is `RAVEN_ORACLE_DIR=<dir>`. With two
optimisation levels the tool also requires their export sets to be identical.

`adhoc` builds one file as family `adhoc-<stem>` without a recipe; its exports are whatever `nm`
reports.

## Recipes

One file per family: `tests/libc/oracle/recipes/<family>/oracle.toml`. Schema `1`; unknown keys are
errors; `family` must match the directory; `exports` must be sorted and unique; `source.path` is
relative, has no `..` and ends in `.c` (assembler is refused); `donor` must be a key of
`donors.lock`; flags are limited to the whitelist of guide 6.2 (optimisation only through `opt`,
includes and defines only through their keys). `defaults.toml`, next to `recipes/`, neutralises
the donor macro idioms of guide section 9.

## Licence gate and pin check

Before compiling, `gate` refuses any file containing `All advertising materials mentioning features`,
`GNU General Public`, `GNU Lesser General Public`, `Common Development and Distribution` or
`BEER-WARE`, and refuses a file whose first 60 lines have no `Copyright`, `Permission`,
`Public domain` or `public domain` text. A musl file without a per-file header is accepted when
`<resources>/musl/COPYRIGHT` exists (H-1). The gate is a safety net; the human gate of guide 1.3
comes first.

`pin` requires `git -C <resources>/<donor> rev-parse HEAD` to equal the `donors.lock` sha and
`git -C ... status --porcelain -- <path>` to be empty. `--no-pin-check` exists for fixtures and
non-git trees only.

## Vendoring donor test C (H-10)

`vendor` walks `<resources>/<donor>/<from>` recursively in sorted order, skips hidden entries and
classifies each file by its first 80 lines: `bsd` (`Redistribution and use`; `BSD-3-Clause` when
`Neither the name` occurs, else `BSD-2-Clause`), `isc` (`Permission to use`), `pd`
(`Public domain`/`public domain`) or an allow-listed `SPDX-License-Identifier`
(`BSD-2-Clause BSD-3-Clause ISC MIT Apache-2.0`). A reject phrase or an unclassified `.c`/`.h` file
is refused and listed in `<to>/REVIEW.txt`; the O-B6 default is to exclude it.

Only `.c`/`.h` files are copied by default. Other files (Makefiles, `*.in`, `*.out`, `*.txt`) are
copied only with `--data` and only when their directory has an accepted source file; an
unclassified data file gets `license = "NOASSERTION"`. Accepted files are copied byte-identically and
get one `[[import]]` entry each in `<to>/provenance.toml` (`tier = "A"`, `modified = false`,
`copied = <--date>`, notes naming H-10). `--dry-run` prints the class table and writes nothing;
`--date` is required otherwise.

`--to` is prepended verbatim to every `[[import]]` path, so call it with a repository-relative
directory (C71 uses `tests/libc-oracle/vendor/...`). Vendored C lives in that add-on directory, is
never linked into a shipped artifact, and is not part of this tool's own test fixtures.
