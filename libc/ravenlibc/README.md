<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# ravenlibc

RavenBSD's C library, written in Rust: one ABI, one source tree, profiles
instead of a second smaller libc. This directory is a plain directory, not a
Cargo package; the workspace glob lives in the root `Cargo.toml`.

| Path | Owner | What |
|---|---|---|
| `crates/` | C34 | `raven-libc-*` crates; one directory per package |
| `xtask/` | C34 | manifests and `libc.mk` |
| `tools/lint/` | C34 | rule table for `libc-lint` |
| `tools/hdrgen/` | C36 | ABI header generator |
| `tools/mkmap/` | C60 | symbol-map tool |
| `abi/` | C36 | ABI census data |
| `include/`, `arch/` | C37 | public headers and arch headers |
| `crt/` | C61 | startup objects |
| `ld/` | C62 | dynamic linker |
| `stubs/` | C66 | personality stubs |
| `tests/` | C73 | target tests |
| `world.toml` | C72 | port manifest |
| `GATE-REPORT.md` | C74 | final evidence |

Layers: L0 pure primitives, L1 pure-logic library, L2a syscall boundary, L2b
platform services, L2c personality sets, L3 shipped ABI shell/loader.

Commands:

```sh
make libc-check
cargo xtask libc-lint [unsafe-scope|cfg|header|provenance-link|no-std|no-cc|layout|all]
cargo xtask libc-oracle   # C35
cargo xtask libc-headers  # C37
cargo xtask libc-pack     # C60
cargo xtask libc-size     # C69
cargo xtask libc-world    # C72
```

Adding a crate: copy `xtask/templates/Cargo.toml.tmpl`, add a row to
`xtask/crates.toml`, run `cargo xtask libc-lint`.

Generated files carry `Generated` in the first ten lines. See
`architecture/raven-libc/00-overview.md` and `17-crate-layout.md`.
