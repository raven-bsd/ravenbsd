<!--
SPDX-License-Identifier: BSD-3-Clause
Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
-->

# raven-libc-fmt

The `printf` and `scanf` engines of ravenlibc. Area document:
`architecture/raven-libc/donor-map/04-stdio.md` (3.1-3.3, 3.8) and
`05-numerics.md` 3.2. Ticket C43.

`vformat` writes to a `Sink`, reads arguments from a `VaReader`, dereferences pointers
through `FmtEnv`, and obtains float digits from a `FloatFmt` hook. `vscan` reads from a
`ScanSource`, stores through `FmtEnv`, and validates float tokens with `FloatScan`. The
crate is `no_std`, `forbid(unsafe_code)`, allocation-free, and has exactly `abi` and `ctype`
as dependencies. It never sees a `FILE`, never touches `errno`, and never contains an
`f32`/`f64` value.

| Module | Contents |
|---|---|
| `types.rs` | `FloatKind`, `FloatBits`, `FloatClass`, `DigitsOut`, `NumericFormat`, `FmtOptions`, `FmtErr`, `ScanOptions`, `ScanResult`, `DEC_BUF_*`, `SCAN_TOKEN_MAX` |
| `traits.rs` | `CharUnit`, `Sink`, `SinkError`, `VaReader`, `FmtEnv`, `FloatFmt`, `FloatScan`, `NoFloat`, `StubFloat`, `NoScanFloat` |
| `printf/` | `vformat`, flags/lengths, integer digits and grouping, strings, the positional table, the float layout, `%n`/`%m` |
| `scanf/` | `vscan`, `Limited`, the integer lexer, the float token lexer, `%[`, `%m`, `%n` |
| `glue/` | `vsnprintf_core`, `vswprintf_core` |
| `sinks.rs` | `CountingSink`, `SliceSink`, `BoundedSink` |

The strong float hook is the shell's job (C60); this crate provides `NoFloat` (integer-only
symbols, `EINVAL`) and `StubFloat` (the `*float*` weak-hook dummy). `vformat::<_, _, _,
NoFloat>` and `::<_, _, _, StubFloat>` reference no numeric symbol.

## Tests

```
cargo test -p raven-libc-fmt
cargo clippy -p raven-libc-fmt --all-targets -- -D warnings
cargo build -p raven-libc-fmt --target x86_64-unknown-none
cargo build -p raven-libc-fmt --target aarch64-unknown-none-softfloat
cargo test -p raven-libc-fmt --test link_separation --release
cargo doc -p raven-libc-fmt --no-deps
```

Unit tests live in the sibling `*_ut.rs` files; `tests/support/` holds `SliceVa`,
`FakeFloat`, `VecSink` and `RecordingSource`. The real `FloatFmt` adapter over
`raven-libc-dtoa` lives in the oracle harness (`tests/libc/oracle/src/support_fmt`).

## Licence

BSD-3-Clause for the crate; the transpiled files carry the FreeBSD or musl notices in their
headers and are recorded in `PROVENANCE.toml`. `netbsd:lib/libc/stdio/vfwscanf.c` (BSD-4)
was not read or copied; the wide scanf code is the same generic engine over `u32`.
