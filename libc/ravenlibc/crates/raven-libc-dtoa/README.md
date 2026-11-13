<!--
SPDX-License-Identifier: BSD-3-Clause
Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
-->

# raven-libc-dtoa

The float/decimal conversions of ravenlibc: the correctly rounded `strtod`, `strtof` and `strtold`
family and the `dtoa`, `ldtoa`, `hdtoa` and `hldtoa` digit generators that `printf` and `scanf`
consume (`architecture/raven-libc/donor-map/05-numerics.md` 3.1-3.2).

Pure computation over caller-provided buffers: no `errno`, no locale lookup, no threads, no process
globals. Every conversion uses a `DtoaCtx` the caller places on its stack; the arena has 8 KiB and
spills to `alloc::vec::Vec` only with the `alloc` feature (otherwise `DtoaError::NoMemory`).

Contains software by David M. Gay (Lucent Technologies), see file headers.

Run the tests with `cargo test -p raven-libc-dtoa` (and `--features alloc`); the C differential is
`cargo xtask libc-oracle diff dtoa` from the repository root.
