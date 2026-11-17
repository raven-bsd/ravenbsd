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

The script is bounded by `tests/libc/miri/crates.txt` (the gate 1.6 crate list), `--lib` only, the
deterministic skips in `tests/libc/miri/skip.txt`, and `MIRI_TIMEOUT` (default 1800 s per crate).
Logs and the resolved toolchain are written under `build/libc-miri/`. Exit 77 means cargo-miri is
not installed.

## Crates and bounded exclusions

The gate crate set: `raven-libc-str`, `raven-libc-fmt`, `raven-libc-dtoa`, `raven-libc-regex`,
`raven-libc-glob`.

`--lib` excludes integration test binaries. `raven-libc-str/tests/alignment.rs` cannot run under
Miri: it exercises the x86_64 inline-assembly `memcpy` at
`libc/ravenlibc/crates/raven-libc-str/src/raw/arch/x86_64/mem.rs:62` and a guard-page `mmap`; Miri
reports `unsupported operation: inline assembly is not supported`. That is a Miri capability
limit, not a UB finding. The integration test stays in the normal nextest gate.

The skip list removes deterministic long vectors (100k/20k iterations) that take hours under the
interpreter. They are not UB findings; nextest runs them on every change. No failing unsafe code
was skipped.

## Result (OBSERVED, 2026-11-17)

| Crate | Tests | Filtered | Result |
|---|---:|---:|---|
| `raven-libc-str` | 160 | 3 | pass, 1290.59 s |
| `raven-libc-fmt` | 124 | 0 | pass, 21.02 s |
| `raven-libc-dtoa` | 60 | 2 | pass, 10.31 s |
| `raven-libc-regex` | 332 | 0 | pass, 83.01 s |
| `raven-libc-glob` | 39 | 0 | pass, 5.32 s |

Total: **715 Miri tests passed, 0 failed, no UB report**. No Miri suppression flags were used
(default isolation, stacked borrows and provenance checks).

Gate row 1.6 result: **PARTIAL**. Miri is installed and the bounded 715-test run is green; the
row's full `rustup run nightly cargo miri test -p ...` command is not green because of the Miri-unsupported
inline-asm `alignment` integration test (test/production-asm owner) and the multi-hour
deterministic vectors. C74w owns `docs/libc-gate.md`/the gate row; this directory is the evidence
for the bounded row and the exact exclusions.

## Files

* `crates.txt` — gate crate set, one package per line.
* `skip.txt` — `<crate> <test-substring> <reason>` deterministic long-vector skips.
