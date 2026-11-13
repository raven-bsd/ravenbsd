<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# `tests/libc/va`: the host tests of the variadic trampolines (C59)

Host-only crate (`std` allowed, system C compiler allowed through `std::process::Command`,
no `build.rs`, no `cc` crate). It builds the C59 module by path
(`src/lib.rs` -> `libc/ravenlibc/crates/raven-libc/src/va/mod.rs`), so
`cargo build -p raven-libc-va-test --target <arch>-unknown-none` is the bare-metal build
check of the module (RD-new 2: the shell skeleton did not exist when C59 started).

## Files

| Path | What |
|---|---|
| `src/lib.rs` | the by-path module; `#![cfg_attr(not(test), no_std)]` |
| `src/arena.rs` | a byte arena addressed by raw pointers, canaries and garbage injection |
| `src/model.rs` | the independent ABI model: `assign(arch, named, args) -> Frame`, `build_list` |
| `src/driver.rs` | the generated-C driver: one function per argument shape, per-case functions, the soft-float variant, `dlopen`/`dlsym` (no `libloading`) |
| `src/targets.rs` | the test `v*` targets: the real engine through `CEnv` (`t_vsnprintf`, `t_vsscanf`), the 40 recorder targets, `t_rec` for aggregates |
| `src/table_ut.rs` | the one expansion of `for_each_variadic!(_, "t_")` |
| `src/*_ut.rs` | sibling unit tests |
| `c/driver_prelude.h` | first-party C prelude of the generated driver (test material only) |
| `corpus/k03.toml` | the 57 K03 cases as data |
| `golden/*.txt` | `llvm-objdump -d` of the `t_snprintf` instance per target and profile |
| `not-variadic-exports.txt` | census names that are not rows of the table, with the reason |

## Tests

| Test | Asserts |
|---|---|
| `reader_sweep` | every class at every position 0..=20 for every `named`, all three (four) models; 5,000 random sequences per model; `clone_state`; no write-back; canaries |
| `k03_corpus` | the 57 K03 cases through the real `t_snprintf` trampoline and the real engine against glibc |
| `diff_x86_64` | layers A-I: >= 31,000 differential cases against glibc, 0 mismatches |
| `diff_native` | the file the `qemu-user` lane runs; natively on x86_64 |
| `softfloat` | the `ranen_va_softfloat` profile against a `clang -msoft-float -mno-sse` caller |
| `golden` | the disassembly of the `t_snprintf` instance per target equals `golden/*.txt` (`RL_UPDATE_GOLDEN=1` regenerates) |
| `failing_stub` | `aarch64`/`riscv64` non-`v` forms are link-time failing stubs until the reviewer's `raven_va_verified_*` flag |
| `table_rows` | every row of C59 section 4.6 is instantiated and its recorder receives `named` words and a reader at argument `named` |
| `no_forbidden` | no `f32`/`f64`/`core::fmt`/`format_args`/`alloc`/`dyn`/`c_variadic`/feature attribute in the module |

## Commands

```text
cargo test -p raven-libc-va-test
cargo build -p raven-libc-va-test --target x86_64-unknown-none
cargo build -p raven-libc-va-test --target aarch64-unknown-none-softfloat
cargo build -p raven-libc-va-test --target riscv64gc-unknown-none-elf
RUSTFLAGS='--cfg raven_va_softfloat' cargo test -p raven-libc-va-test --test softfloat
```

The generated C and the shared objects live under `target/` (via `CARGO_TARGET_TMPDIR`) and
are never committed. If `clang` is missing, the differential tests print
`skipped: clang not found` and pass; the report says so.

## Documented differences from the glibc reference

* `%p` of NULL: glibc prints `(nil)`, C43/FreeBSD print `0x0` (the K03 case is marked
  `exclude` in the corpus).
* `scanf` integer overflow is undefined by C: glibc wraps, the musl engine saturates, so the
  out-of-range inputs are excluded from layer I.
