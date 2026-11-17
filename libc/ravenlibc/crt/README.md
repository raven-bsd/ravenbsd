<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# raven-libc-crt

The start files of ravenlibc, built as Rust `global_asm!` objects without a C compiler
(`donor-map/10` 3.12, `donor-map/14` 3.A.8). `xtask libc-pack` compiles the crate as a
staticlib and extracts one archive member per object into `build/<arch>/`:

| Object | Entry | `sp0` | `rtld_fini` | `iplt_start`, `iplt_end` | `main` |
|---|---|---|---|---|---|
| `crt1.o` | static, non-PIE | `rdi` = `rsp` | 0 | `__rela_iplt_start`, `__rela_iplt_end` | `r8`/`x4`/`a4` |
| `Scrt1.o` | dynamic PIE | `rdi` = `rsp` (loader jump) | x86-64 `rdx`, AArch64 `x1`, RISC-V `a1` | 0, 0 | `r8`/`x4`/`a4` |
| `rcrt1.o` | static-PIE | `rdi` = `rsp` | 0 | 0, 0 | `r8`/`x4`/`a4` |

The fifth argument is the executable's `main` address. Passing it from the crt keeps the shared
libc free of a strong undefined `main`, so a Rust PIE with its own `_start` loads without one;
a C program's `main` still gets called.

The other five objects are `crti.o`, `crtn.o` (empty by decision), `crtbegin.o`,
`crtbeginS.o` (the `__dso_handle` fallbacks) and `crtend.o` (the `.eh_frame` terminator).

`cargo test -p raven-libc-crt` runs the host tests: the self-relocator on a fake image, the
structure of the eight produced objects, and H1-H4, which link and execute static and
static-PIE programs natively on a Linux x86-64 host.

**`selfreloc.rs` and `selfreloc_consts.rs` are included by `ld/` through `#[path]`: keep them
dependency-free, with no `crate::` paths, no statics and no indexing.**
