<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# libc symbol audit

Status: C64j host-side inventory, wired to the build system by C64l. It reads the linked
`libc.so.1` and the seven C73c target case sources; it does **not** boot QEMU or claim target
behaviour. The current snapshot is `tests/libc/symbol-audit/expected.tsv`; the matcher is
`scripts/libc-symbol-audit.sh`.

## Run

```
cargo xtask libc-audit             # C74 gate command; rebuilds libc when the export file is absent
cargo xtask libc-audit --rebuild   # force pack/link, then audit
cargo xtask libc-audit --self-test # fake tables, no libc/toolchain needed

sh scripts/libc-symbol-audit.sh --rebuild   # pack/link libc, then audit
sh scripts/libc-symbol-audit.sh             # use build/x86_64/libc.so.1
sh scripts/libc-symbol-audit.sh --self-test # fake tables, no libc/toolchain needed
```

Exit codes: 0 all required symbols exported; 1 one or more missing (the matrix is still written to
`build/libc-symbol-audit/matrix.tsv`); 2 usage/I/O/missing host tool. The current tree exits 1.
`cargo xtask libc-audit` forwards the same exit code; `cargo xtask libc-audit --self-test` exits 0
on the fake tables.

## C74 gate wiring

`tests/libc/gate/rows.toml` has the check:

```toml
[[check]]
id = "1.1.audit"
row = 1
title = "target-case symbol audit (C64j)"
required = true
kind = "argv"
argv = ["cargo", "xtask", "libc-audit"]
evidence = "build/libc-gate/1.1.audit.txt"
source = "C64j, C64l"
```

`cargo run -q -p libc-gate -- eval --only 1.1.audit` writes
`build/libc-gate/1.1.audit.txt` and marks the check MET only when
`cargo xtask libc-audit` exits 0. The current 18 missing symbols make it NOT MET
until C64i lands. The `libc_audit_ut.rs` host test parses `tests/libc/gate/rows.toml`
and asserts the row's argv is this command.

The matrix columns are `case<TAB>symbol<TAB>status<TAB>owner<TAB>note`, sorted by case then symbol.
`required.tsv` is the per-case inventory derived from the seven C sources under
`libc/ravenlibc/tests/c/` and the C73c section 3 link failures. `expected.tsv` is the current
snapshot; update it only after a fix changes a status, and say so in the ticket report.

## Current delta (OBSERVED)

`sh scripts/libc-symbol-audit.sh` reports:

```
libc-symbol-audit: required=66 present=48 missing=18
```

`cargo xtask libc-link --arch x86_64 --check` reports
`mkmap: check: ok K1..K7 (1828 members, 1091 shell, 1133 public, 27 private)`; the 18 missing
names are not in `Symbols.list` or `Symbol.map` and are not exported by `libc.so.1`.

| Group | Symbols | Owner |
|---|---|---|
| stdio | `__ctype_get_mb_cur_max`, `fgetwc`, `fwprintf`, `getline`, `remove`, `rewind`, `stderr`, `stdin`, `stdout` | C64i (C55/C56 stdio donors) |
| pty/termios | `grantpt`, `posix_openpt`, `ptsname_r`, `tcgetattr`, `tcsetattr`, `unlockpt` | C64i (C55/C56 pty donors) |
| random | `arc4random_buf` | C64i (C50 history) |
| string/stdio | `fgetln` | C64i |
| process | `umask` | C64i |

C64i owns `libc/ravenlibc/crates/**`, `libc/ravenlibc/abi/**` and the ABI map; this ticket does not
edit them. The 48 present rows cover the symbols the cases use from hello, time, threads, signals,
spawn and the loader.

## What the matrix does not claim

The audit is a symbol-presence check. It does not replace the target runs:

* `libc-hello` still has the `auxv`/`string` behaviour failures (PG-50/startup and shell string
  owner).
* `libc-time` is build-blocked by the soft-float/double ABI, not by a missing export.
* `libc-threads`/`libc-signals` fail at runtime (page faults), not at link time.
* `libc-spawn` fails in the libc `posix_spawn`/`waitpid` backend (`LocalBackend::new(None)`), not
  in an exported symbol; B71a records that follow-up.
* The `stdio`/`dynamic` cases remain build-blocked by the 18 missing names above until C64i lands.

## Self-test

`tests/libc/symbol-audit/self-test/` contains a fake required table, an all-present export table, a
one-missing export table and the two expected matrices. `--self-test` checks the all-present exit
code (0), the missing exit code (1), the matrix contents and a second-run determinism check. It uses
no libc, compiler or QEMU.

## Evidence tags

* OBSERVED: the 66/48/18 count, the per-case source references and the current `nm -D` status.
* OBSERVED: `libc-link` K1..K7 and the 1133/27 public/private split.
* PROPOSED: the owner column (C64i for all 18 missing groups; C50 is only the historical random
  owner).
* UNVERIFIED: any target run after C64i adds the exports.
