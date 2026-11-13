<!--
SPDX-License-Identifier: BSD-3-Clause
Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
-->

# raven-libc-str

The `<string.h>`, `<strings.h>` and `<stdbit.h>` family of ravenlibc (area 01): safe slice engines
(`mem`, `cstr`, `secure`, `bits`, `stdbit`, `strmode`, `errmsg`, `gnu_basename`, `itoa`) and one
audited pointer module, `raw`, that the C-ABI shell (C60) wraps in `extern "C"`. The crate is
`no_std`, allocation-free, `#![no_builtins]`, deterministic and free of `core::fmt`.

Build and test: `cargo test -p raven-libc-str`; the recursion gate is
`cargo test -p raven-libc-str --features test-probes --test no_recursion`, the constant-time
structure is `--test secure_structural`, and the guard-page sweeps are `--test alignment`.
Oracle families: `cargo xtask libc-oracle diff str-mem|str-openbsd|str-freebsd|str-musl|str-errmsg`.

Deferred (doc 01 5.8): FORTIFY `__*_chk` (C50), FreeBSD `amd64/*.S`, aarch64 MOPS/SVE variants,
`riscv/*.S`, bionic RVV, any dispatch table/IFUNC (H-17), `argz`/`envz`/`strfry`, Annex K beyond
`memset_s`, `sys_errlist`/`sys_siglist`, `strerror_ss`, wide `wcs*`.

The `aarch64` routines imported from the Arm optimized routines read whole aligned 16-byte blocks
and are **not MTE compatible** (`strlen.S:11`); do not enable MTE for the heap while they are the
active implementations. They are compiled only for a NEON-capable aarch64 target; the soft-float
target uses the generic Rust path.
