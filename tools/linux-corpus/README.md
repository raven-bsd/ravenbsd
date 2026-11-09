# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

# linux-corpus

Host tool that runs a curated Linux program list under `strace -f`, parses the
traces, and writes the syscall-coverage dashboard used to prioritise the Linux
personality work (`architecture/09-linux-compat.md` section 5).

## Commands

```
cargo run -q -p linux-corpus -- run [--corpus tools/linux-corpus/corpus.toml]
    [--out tools/linux-corpus/out] [--only <id>...]
cargo run -q -p linux-corpus -- merge [--out tools/linux-corpus/out]
    [--json docs/linux-syscall-coverage.json] [--md docs/linux-syscall-coverage.md]
    [--implemented <file>]
cargo run -q -p linux-corpus -- missing --implemented <file> [--out tools/linux-corpus/out]
```

`run` exits 0 after a completed run even when individual programs fail or are
skipped; the per-program JSON and the dashboard record the failures. Usage and
I/O errors exit 2. `merge` writes both dashboard files. `missing` prints the
corpus syscalls absent from a one-name-per-line implemented list, ranked by the
number of programs that use them.

## Corpus format

`corpus.toml` has `[[program]]` tables with `id`, `category`
(`static|dynamic|service|language|toolchain|shell|net`), `argv`, optional
`requires` (binaries that must exist), `setup` (shell-free argv arrays), `stdin`,
`env`, `expect_exit` (default 0) and `timeout_secs` (default 30). `{work}` is a
fresh per-program scratch directory; `{corpus}` is the directory containing the
corpus file. Missing `requires` binaries are reported as
`skipped: not installed`.

Each program runs as:

```
strace -f -qq -s 64 -o <out>/<id>.strace <argv>
```

with a process-group timeout. The trace is parsed once; the per-program JSON
contains the syscall aggregates and the runner's exit/duration data.

## Syscall numbers

`src/x86_64_nr.rs` is generated from the donor's generated Rust constants, not
from Linux kernel source or memory:

```
tools/linux-corpus/gen-x86_64-nr.sh \
  ../ravenbsd-resources/linux-raw-sys/src/x86_64/general.rs
```

The generated file header records the `linux-raw-sys` revision and the
Apache-2.0 OR MIT provenance.

## Fixtures

`tests/fixtures/` keeps small synthetic samples from the ticket assets and real
captures from the development host. `out/` and `work/` are local run artefacts
and are not committed.
