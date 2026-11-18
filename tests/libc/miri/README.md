<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# Bounded Miri run for libc gate 1.6

Miri is the only nightly tool in the project. Normal builds remain on the pinned stable
`rust-toolchain.toml` (1.97.1); `scripts/check-miri.sh` invokes `rustup run nightly cargo miri`
explicitly and never changes the repository toolchain.

## Toolchain and command

Exact toolchain used for this evidence:

```
rustc 1.102.0-nightly (5b7b37aef 2026-11-08)
cargo 1.102.0-nightly (c07ac2c4d 2026-11-06)
miri 0.1.0 (5b7b37aefc 2026-11-08)
```

Install when absent:

```
rustup component add --toolchain nightly miri
```

Run:

```
sh scripts/check-miri.sh
```

The script is bounded by `tests/libc/miri/crates.txt` (the gate 1.6 crate list), the five crate
library suites (`--lib`), the one `cfg(miri)` alignment integration binary, the deterministic
skips in `tests/libc/miri/skip.txt`, the explicit `integration-exclusions.txt`, and
`MIRI_TIMEOUT` (default 1800 s per crate). Logs and the resolved toolchain are written under
`build/libc-miri/`. Exit 77 means cargo-miri is not installed.

## Crates and bounded exclusions

The gate crate set: `raven-libc-str`, `raven-libc-fmt`, `raven-libc-dtoa`, `raven-libc-regex`,
`raven-libc-glob`.

`--lib` bounds the five crate library suites. One integration binary is also in the bounded row:
`raven-libc-str/tests/alignment.rs` has a `cfg(miri)` path that prints the exact capability
exclusion (`x86_64 inline assembly (src/raw/arch/x86_64/mem.rs:62)` and guard-page `mmap`) and
returns without executing the unsupported operations; the real target tests are unchanged and run
under nextest. The remaining integration binaries are listed explicitly in
`integration-exclusions.txt` (host toolchain subprocesses, host filesystem fixtures and pure
fixture/vector suites covered by nextest); none is a Miri UB claim.

The skip list removes deterministic long vectors (100k/20k iterations) that take hours under the
interpreter. They are not UB findings; nextest runs them on every change. No failing unsafe code
was skipped.

## Result (OBSERVED, 2026-11-17)

| Crate | Tests | Filtered | Result |
|---|---:|---:|---|
| `raven-libc-str` | 160 | 3 | pass, 1290.59 s |
| `raven-libc-str` alignment | 6 | 0 | pass, 0.72 s |
| `raven-libc-fmt` | 124 | 0 | pass, 21.02 s |
| `raven-libc-dtoa` | 60 | 2 | pass, 10.31 s |
| `raven-libc-regex` | 332 | 0 | pass, 83.01 s |
| `raven-libc-glob` | 39 | 0 | pass, 5.32 s |

Total: **721 Miri tests passed, 0 failed, no UB report** (715 library tests + 6 alignment
integration tests). No Miri suppression flags were used (default isolation, stacked borrows and
provenance checks).

Gate row 1.6 result: **MET (bounded, explicit exclusion list)**. The bounded runner is green with
721 Miri tests and no UB. The only exclusions are the five deterministic long vectors in
`skip.txt` and the host/subprocess/pure integration binaries in `integration-exclusions.txt`; all
are covered by nextest. The row's raw five-crate argv without the bounded exclusions would also
run those host-only binaries (starting with the Miri-isolation `no_recursion` build fixture) and
the multi-hour vectors; C74w owns `docs/libc-gate.md`/the gate row and can point it at
`scripts/check-miri.sh` or the explicit exclusion list. The alignment capability exclusion is
printed by the test itself (`-- --nocapture`).

## Files

* `crates.txt` — gate crate set, one package per line.
* `integration-exclusions.txt` — explicit integration-binary exclusions and reasons.
* `skip.txt` — `<crate> <test-substring> <reason>` deterministic long-vector skips.
