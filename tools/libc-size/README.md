# libc-size

<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

Host tool for the ravenlibc size gate (`donor-map/16` Part A, ticket C69). It builds the
probes P0-P9 for `x86_64`, `aarch64` and `riscv64`, measures the stripped executables
(`S = text + data`, `B = bss`, `F = file bytes`), evaluates the probe rules and the hard
invariants, and compares the result with `tests/libc/size/BUDGETS.toml`.

```sh
cargo run -q -p libc-size -- run    --arch x86_64 --linkage static --set tiny --lang rust
cargo run -q -p libc-size -- check  --results build/size/libc-size.json
cargo run -q -p libc-size -- freeze --results build/size/libc-size.json
cargo run -q -p libc-size -- diff   --old old.json --new build/size/libc-size.json
cargo run -q -p libc-size -- members --map build/size/x86_64/tiny/static/rust/p2.map
cargo run -q -p libc-size -- boot   --arch x86_64
cargo run -q -p libc-size -- relocs --so build/x86_64/libc.so.1
cargo run -q -p libc-size -- monomorph --lib build/x86_64/libc.a
```

`cargo xtask libc-size ARGS` forwards verbatim; `--check` is the shorthand for `run` followed
by `check`.

## Output

`build/size/libc-size.json` and `build/size/libc-size.md` are deterministic: `generated` is
`null`, no timestamp and no host path appears, rows are sorted by
arch -> linkage -> set -> language -> probe. The per-probe binaries and maps stay under
`build/size/<arch>/<set>/<linkage>/<lang>/`. The committed baseline is
`tests/libc/size/results/`.

## Skips and exit codes

Exit codes: `0` ok, `1` a gate failed, `2` usage or I/O, `77` a required tool is missing
(`skipped: <tool> not found`, C35 style; `RL_SIZE_REQUIRED=1` turns it into a failure). A probe
that cannot be built is a `skipped: <reason>` row and is never counted as a pass. While C62
has not landed every dynamic and `libc.so.1` row says exactly
`skipped: libc.so.1 absent (C62 not landed)` and records the path it looked for.

## Tool discovery

`LLVM_SIZE LLVM_NM LLVM_READOBJ LLVM_OBJDUMP LLVM_STRIP LLVM_AR LLVM_OBJCOPY LD_LLD`
(`CLANG` optional) override the lookup; otherwise
`$(rustc --print sysroot)/lib/rustlib/<host>/bin`, then `PATH`. `ld.lld` is also looked up in
that directory's `gcc-ld/`. Subprocesses always use an argument vector, never a shell.

## Layout

| File | Role |
|---|---|
| `src/main.rs` | CLI and the run/check/freeze/diff/members/boot/relocs/monomorph commands |
| `src/probe.rs` | P0-P9 table and the four link recipes |
| `src/build.rs` | subprocess steps and `libc-pack` prerequisite |
| `src/measure.rs` | `llvm-size`/`llvm-nm`/`llvm-readobj`/map parsers |
| `src/rules.rs` | probe rules and hard invariants |
| `src/budget.rs` | freeze, check, `known-violation` |
| `src/boot.rs` | BP-1..BP-5 static boot checks |
| `src/relocs.rs` | `libc.so.1` size and relocation rows |
| `src/monomorph.rs` | R-10 generic instantiation counter |
| `src/members.rs` | member matching and R-30 table |
| `src/report.rs` | JSON/Markdown writer |

Tests are sibling `_ut.rs` files; `cargo test -p libc-size` runs them on the host.
