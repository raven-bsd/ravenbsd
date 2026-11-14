<!--
SPDX-License-Identifier: BSD-3-Clause
Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
-->

# ravenlibc fuzz targets

The libc parsers read bytes the program did not write. This directory holds one stable replay
crate (`replay/`), one nightly `cargo-fuzz` crate (`targets/`, its own workspace), the seed
lists, the generated corpora and the finding regressions.

## Stable replay

```sh
cargo test -p raven-libc-fuzz-replay
cargo run -q -p raven-libc-fuzz-replay --bin replay -- list
cargo run -q -p raven-libc-fuzz-replay --bin replay -- run printf_format corpus/printf_format/seed-001
```

The replay crate holds every target body, so a crash is an ordinary stable test case. It runs
each input under a 10-second watchdog and a 64 MiB peak-allocation cap (256 MiB for
`regex_compile_exec`), replays every corpus and regression file, then sweeps 2,000 seeded inputs
per target (`RL_FUZZ_SWEEP` overrides the count).

## Tooling

`cargo-fuzz` and libFuzzer are developer tools, not dependencies of a shipped crate.

```sh
cargo install cargo-fuzz --locked
rustup toolchain install nightly
```

The scripts use `rustup run nightly cargo fuzz ...`; the stable guard rejects the
toolchain selector syntax in shell files.

## Smoke and soak

The crate directory is named `targets/`, so the scripts pass `--fuzz-dir targets` (the
cargo-fuzz default is a directory named `fuzz`).

```sh
sh smoke.sh                 # 60 s per target
sh smoke.sh printf_format   # one target
sh soak.sh printf_format 1  # 1 hour
```

`smoke.sh` prints one `smoke <target>: ok|CRASH` line per target and exits 77 when the tool is
absent. `soak.sh` writes only under `$TMPDIR`; never commit its corpus or artifacts.

## Corpus and seedgen

`seeds/<target>.txt` holds human-readable seeds (`\xNN`, `\\`, `#` comments). `seedgen` turns
them into `corpus/<target>/seed-NNN`, adds the synthetic TZif/ELF images and the optional
Spencer regex cases, and is deterministic:

```sh
cargo run -q -p raven-libc-fuzz-replay --bin seedgen -- --out tests/libc/fuzz/corpus
```

Every corpus directory has 10-300 files of at most 1 KiB; the total is under 2 MiB. The seed
provenance is in `corpus/SOURCES.md`.

## Triage

A panic, timeout, memory-cap hit or differential mismatch is minimised and stored as
`regressions/<target>/<YYYY-MM-DD>-<owner>-<slug>.bin` with a `.md` note. The owning crate gets
a stable test when its ticket is done and the change is test-only. A finding keeps the replay
test red until the owner fixes it; it is never hidden by weakening a property or a limit.

## Property ids

Every assertion in a body carries `P-<target>-<n>`; the mutation-proof test of each target uses
`#[should_panic(expected = "P-<target>-<n>")]`. The DEFERRED mechanism is described in
`known-gaps.md`.
