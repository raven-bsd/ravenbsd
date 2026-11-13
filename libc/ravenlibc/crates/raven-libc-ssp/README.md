<!--
SPDX-License-Identifier: BSD-3-Clause
Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
-->

# raven-libc-ssp

The stack-protector runtime and FORTIFY check cores (area 08 3.15-3.16). `guard` seeds one
`usize` canary from `AT_RANDOM` (or the terminator fallback), clears byte 1 and writes both the
global and TLS copies through `CanarySink`; `fail` runs the block/log/reset/raise/exit-127
sequence through `FailHooks`; `overlap` is the `__ssp_overlap` arithmetic; `chk` contains one
pure check core per FreeBSD `*_chk.c` file. The C-ABI shell owns the exported symbols.

Build and test: `cargo test -p raven-libc-ssp`.
