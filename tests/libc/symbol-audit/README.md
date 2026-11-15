<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# libc symbol-audit fixtures

C64j compares the exported `libc.so.1` dynamic symbols with the symbols the seven C73c target
cases reference. The runner is `scripts/libc-symbol-audit.sh`; C64l adds the build-system command
`cargo xtask libc-audit`.

| File | Purpose |
|---|---|
| `required.tsv` | Per-case required symbols: `case<TAB>symbol<TAB>owner<TAB>note`. Derived from the seven C sources under `libc/ravenlibc/tests/c/` and the C73c section 3 link failures. |
| `expected.tsv` | Current deterministic matrix (`case<TAB>symbol<TAB>status<TAB>owner<TAB>note`); update it after a fix changes a status. |
| `self-test/required.tsv` | Fake required table for the self-test. |
| `self-test/exports-present.txt` | Fake export set with every fake symbol present. |
| `self-test/exports-missing.txt` | Fake export set with one fake symbol absent. |
| `self-test/expected-present.tsv` | Expected all-present matrix. |
| `self-test/expected-missing.tsv` | Expected one-missing matrix. |

The C74 gate row `1.1.audit` in `tests/libc/gate/rows.toml` runs `cargo xtask libc-audit` and
records `build/libc-gate/1.1.audit.txt`; the host test parses that real row.

Run:

```
cargo xtask libc-audit            # gate command; rebuilds libc when the export file is absent
cargo xtask libc-audit --rebuild  # force pack/link, then audit
cargo xtask libc-audit --self-test # fake tables, no libc or compiler needed
sh scripts/libc-symbol-audit.sh --self-test
sh scripts/libc-symbol-audit.sh --rebuild
```

The host-only self-test checks the matcher. The rebuild path packs/links the target libc and writes
`build/libc-symbol-audit/matrix.tsv`; it exits 1 while any required symbol is missing. The current
snapshot is 66 required, 48 present, 18 missing; see `docs/libc-symbol-audit.md` for the groups and
owners. `cargo test -p xtask` runs the same fake-table paths through `libc_audit_ut.rs`. No source
under `libc/ravenlibc/**` or `tests/libc/target/**` is modified by this ticket.
