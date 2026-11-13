<!--
SPDX-License-Identifier: BSD-3-Clause
Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
-->

# raven-libc-oracle

Differential harness for ravenlibc: the donor C original runs next to the Rust subject on the same
inputs, and every observable difference is a failure. The C is built from the read-only donor
checkout at test time by `libc-oracle` (never committed, never linked into a shipped artifact). The
harness is host-only; `errno` observation is Linux-only (`__errno_location`).

Ticket C35; `architecture/raven-libc/05-transpile-guide.md` sections 6.1-6.7 and
`donor-map/16-size-testing-oracles.md` 3.B.2-3.B.5.

## Commands

```
cargo test -p raven-libc-oracle                     # Rust-only: RNG, config, diff, stand-ins, mutants
cargo xtask libc-oracle diff string-basic           # build O2 + O0 and run the C-oracle tests
cargo xtask libc-oracle diff string-basic --sanitize
cargo xtask libc-oracle build string-basic
```

`diff` sets `LIBRARY_PATH` and `LD_LIBRARY_PATH` to the directory printed by the build and runs
`cargo test -p raven-libc-oracle --features oracle-<family>`. For a manual run:

```
cargo run -q -p libc-oracle -- build string-basic --out build/libc-oracle
LIBRARY_PATH=$PWD/build/libc-oracle/string-basic/O2 \
LD_LIBRARY_PATH=$PWD/build/libc-oracle/string-basic/O2 \
  cargo test -p raven-libc-oracle --features oracle-string-basic
```

## Environment variables

| Variable | Default | Meaning |
|---|---|---|
| `RL_DIFF_SEEDS` | 20 | random seeds per function |
| `RL_DIFF_CASES` | 10000 | cases per seed |
| `RL_DIFF_SEED_BASE` | 1 | first seed |
| `RL_DIFF_EXHAUSTIVE` | 0 | also run exhaustive domains that are not marked small |
| `RL_DIFF_FILTER` | unset | only run tests whose name contains the value |

Malformed values fall back to the default and print a warning. The random stream is xoshiro256**
seeded through SplitMix64; the reference vectors live in `src/rng_ut.rs`, so every failure is
reproducible from the printed seed and index.

## What a comparison observes

`Case::Obs` is compared whole: the return value and every byte of every output buffer, including
the guard bytes after `dsize`. A function that sets `errno` wraps the call in
`norm::with_errno(|| ...) -> (R, i32)`, which clears the errno slot, calls and reads it back.
Nothing is normalised away except host differences declared in a case type; a loosened comparison is
never the answer.

Each `diff_case!` runs three classes in order:

1. **fixed**: `corpus/<function>/<name>.toml`;
2. **random**: `RL_DIFF_SEEDS` x `RL_DIFF_CASES`, seed printed in every reproducer;
3. **exhaustive**: small domains (fewer than 1,000,000 cases) by default, larger only with
   `RL_DIFF_EXHAUSTIVE=1`.

At most ten mismatches per class are recorded (all cases are still counted). Each recorded case is
shrunk greedily (at most 200 steps). The printed reproducer is a corpus entry:

```
[[case]]  # raven-libc-oracle mismatch: fn=memchr class=random seed=7 index=3712
c = 128
bytes_hex = "7f80"
# oracle: ret = 1
# rust:   ret = -1
```

## When a mismatch is printed

1. Paste the `[[case]]` body into the function's corpus file and confirm it reproduces.
2. Decide which side is right using the area file. **The C wins** unless the area file says the Rust
   deviates on purpose.
3. If the Rust is wrong, fix it; the corpus entry stays.
4. If the C has undefined behaviour and the area file chooses a different behaviour, the deviation
   becomes a `// RavenBSD:` comment at the spot, a corpus entry, and a `normalise` rule in the case
   type. Never loosen the comparison for the sake of passing.

## Layout

```
src/rng.rs          xoshiro256**/SplitMix64
src/config.rs       RL_DIFF_* parsing
src/diff.rs         classes, generators, run_diff, Report, diff_case!
src/corpus.rs       corpus loader and bytes/bytes_hex parsing
src/shrink.rs       greedy shrinker
src/call.rs         case types, generators, unsafe oracle calls, errno
src/norm.rs         with_errno
src/standin/        clean-room subjects (the mutation base)
src/mutant/         eight deliberately broken variants and the self-oracle drill
src/decls/          extern "C" declarations with the #[link] attribute
corpus/             fixed cases
recipes/            one oracle.toml per family
shim/               replacement headers, written from scratch
tests/              C-oracle differential and mutation tests
```

## Adding a family (C40-C53 checklist)

1. `recipes/<family>/oracle.toml` with sorted `exports`, sources and (only if needed) whitelisted
   flags.
2. `[features] oracle-<family> = []` and add it to `all-oracles`.
3. `src/decls/<family>.rs` with the extern block and `#[cfg_attr(feature = "oracle-<family>",
   link(name = "oracle_<family>", kind = "dylib"))]`, plus the `mod` line in `decls/mod.rs`.
4. `tests/diff_<family_underscored>.rs` with `#![cfg(feature = "oracle-<family>")]` and one
   `diff_case!` entry per function.
5. Corpus files under `corpus/<function>/`.
6. A mutation test next to the real test for every function whose branches are not obviously
   covered; the fixed corpus should contain a case for each branch, including high bytes and
   arguments outside a byte.
7. `cargo xtask libc-oracle diff <family>` green at O2 and O0, then `cargo clippy -p
   raven-libc-oracle --all-targets --features oracle-<family> -- -D warnings`.

Mutation tests are not optional: `tests/mutation_string_basic.rs` proves that the same generators
kill the eight broken stand-in variants against the C oracle. A harness that cannot fail is worse
than no harness.
