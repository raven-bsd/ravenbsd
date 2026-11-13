# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

# raven-libc-hdrgen

Host tool that turns `libc/ravenlibc/abi/*.toml` into the Rust crate `raven-libc-abi` and the
constant parts of the C headers. Doc 14 3.B.1; the binary name used by the make flow is
`raven-libc-hdrgen`. Host `std`, `#![forbid(unsafe_code)]`, no `build.rs`, no `cc`.

Dependencies: `anyhow`, `serde` (derive), `toml`, `raven-linux-abi` (syscall tables only).
Temporary directories live under `std::env::temp_dir()` and are removed on drop.

## Commands

```sh
raven-libc-hdrgen generate   [--abi DIR] [--rust DIR] [--root DIR] [--resources DIR]
raven-libc-hdrgen check      [same paths]
raven-libc-hdrgen verify     [--target x86_64|aarch64|riscv64|all] [--resources DIR]
                             [--clang CLANG] [--sed SED] [--root DIR]
raven-libc-hdrgen provenance --date YYYY-MM-DD [--resources DIR] [--root DIR]
```

Exit codes: `0` success, `1` a check failed (invalid tables or a failing probe), `2` usage or
I/O, `77` a required tool (`clang`, `sed`) is missing and `RL_ABI_REQUIRED` is unset. A
`verify` run without the musl checkout is also `77` unless `RL_ABI_REQUIRED=1`.

`--root` defaults to the current directory; `--abi` and `--rust` default to
`libc/ravenlibc/abi` and `libc/ravenlibc/crates/raven-libc-abi/src/generated` under it.
`--resources` defaults to the nearest `ravenbsd-resources/donors.lock` at or above the root;
the musl and `linux-raw-sys` revisions in the generated notices are read from that lock, never
typed.

## Verification design (doc 14 4.7)

1. **L1** — `layout.rs` computes `repr(C)` offsets from the field lists and rejects a table
   whose stated `off`, `size` or `align` differs, naming the record and field.
2. **L2** — `verify` writes a temporary C file with the same structs and
   `_Static_assert(sizeof/align/offsetof)`, then runs clang for each target.
3. **L3** — the generated Rust const assertions compile for the three bare targets.
4. **Musl probe** — builds the musl include tree per arch (`sed -f mkalltypes.sed...`), measures
   every `musl` row, prints the table and requires the measured difference set to equal
   `abi/deviations.toml` exactly (no missing row, no stale row).
5. **H1/H2/H3** — compiles every musl-derived constant against the x86_64 tree (H1), scans the
   source files with the group's `prefixes` for unlisted constants (H2), and proves the
   aarch64 `O_*` difference set is exactly `O_DIRECTORY O_NOFOLLOW O_DIRECT O_LARGEFILE
   O_TMPFILE` (H3).
6. **Compiler identity** — `clang --target=<a>-unknown-ravenbsd -E -dM -x c /dev/null`
   compared with `types.toml` and doc 14 D2-D4 (`char`, `wchar_t`, `wint_t`, `long double`).
7. **C header leg** — every generated header compiles standalone for the three targets with
   `-Werror`, in the default mode and with all visibility classes on.
8. **Syscalls** — `arch/<a>/bits/syscall.h` from `raven_linux_abi::sysnums::{X86_64,
   AARCH64, RISCV64}`; both `__NR_*` and `SYS_*`, sorted by name.

The negative controls live in `tests/mutation.rs`: a flipped offset, a flipped constant, a
changed opaque size, a flipped shift, an undocumented musl difference and a stale deviation
must each fail and name the item.
