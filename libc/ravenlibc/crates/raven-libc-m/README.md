<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# raven-libc-m

`ravenlibc` libm: the double and float math kernels, classification and floating-point
environment. The numeric core is a provenance-recorded transpile of the vendored Rust `libm`
crate (MIT, rust-lang/rust `library/compiler-builtins/libm`), the ARM optimized-routines and
FreeBSD `msun`; the header and behaviour rules are in
`architecture/raven-libc/donor-map/05-numerics.md`.

The crate is `no_std`, has no allocation, and depends only on `raven-libc-abi`. `unsafe` is
confined to `src/arch/` (hardware wrappers) and `src/fenv/` (control registers); the rest is
safe and forbids the recursion trap (no libm function calls the compiler intrinsic it
implements).

Tests run on the host:

```sh
cargo test -p raven-libc-m
cargo test -p raven-libc-m --features soft-float
cargo build -p raven-libc-m --target x86_64-unknown-none
```

Status: partial implementation (exact-class foundation: `support`, `generic`, `portable`,
`coremath`, `fp`); the transcendental, complex, `fenv` and long-double phases are listed in the
C47 report.
