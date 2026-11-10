<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# Building RavenBSD

RavenBSD is built with `cargo` through the top-level `Makefile`. The Makefile forwards to the
Rust `xtask` crate; `make help` lists every target.

## Prerequisites

Run `make setup` once per machine. It installs the host packages, the pinned stable Rust toolchain
from `rust-toolchain.toml`, and records firmware paths for QEMU in `.raven-env.mk`. `make doctor`
reports what is installed or missing.

## Common targets

| Target | Result |
|---|---|
| `make check` | fmt, clippy, host tests, stable guard, provenance, target clippy |
| `make guard` | the stable-Rust guard only |
| `make kernel ARCH=x86_64` | `build/<arch>/kernel.elf` |
| `make loader ARCH=x86_64` | `build/<arch>/loader.efi` |
| `make dev` / `make release` | debug / smallest build of every component that exists |
| `make image PROFILE=base` | bootable image (ticket K05, not implemented yet) |
| `make run PROFILE=shell` | interactive serial QEMU boot; use `Ctrl-A x` to quit |
| `make test` | `make check` plus the QEMU integration tests |
| `make provenance` | SPDX headers, `PROVENANCE.toml` and dependency licences |
| `make clean` / `make distclean` | remove build output / also local caches |

Variables: `ARCH` is `x86_64`, `aarch64` or `riscv64` (default `x86_64`), `PROFILE` defaults to
`base`, `RELEASE=1` selects the release profile, `VERBOSE=1` prints each command xtask runs.

## xtask

`cargo xtask <command>` is equivalent to `make <target>` for the build commands. The important
ones are `check [--fast]`, `guard`, `build <kernel|loader|user> --arch <a> [--release] [-p <pkg>]`,
`provenance`, and the K05 stubs `image`, `initfs`, `run`. `check --fast` skips the per-target
clippy passes.

Components are built with separate cargo invocations and separate target directories
(`build/cargo/<component>-<arch>`) because their flags differ. xtask clears the caller's
`CARGO_ENCODED_RUSTFLAGS` and sets `RUSTFLAGS` explicitly from the CONTRACTS C2 table, so a
developer's environment cannot change the result.

Builds are reproducible: `SOURCE_DATE_EPOCH` is the `HEAD` commit timestamp and
`RAVEN_BUILD_ID` is `<short hash>[-dirty] <YYYY-MM-DD>`. Artifacts land under
`build/<arch>/`; the kernel keeps its symbol table.

## Profiles

`profiles/<name>.toml` lists the programs and files packed into the initial filesystem and the
boot command line. The shipped profiles are `base`, `tiny`, `dev` and `shell`. Unknown keys, a
mismatched `name` field, duplicate paths and invalid log levels are rejected when the profile is
parsed.

Profiles may set `extends = "parent"` to inherit the parent's entries (including its fragments)
and replace its `[boot]` table. One inheritance level is intended; cycles are rejected. A ticket
adds independent shell content in a sorted fragment such as `profiles/shell.d/20-service.toml`:

```toml
[[initfs.programs]]
package = "raven-logd"
path = "/bin/logd"

[[initfs.files]]
src = "services/logd.toml"
path = "/etc/services.d/logd.toml"
mode = 0o644
```

Fragment entries merge in filename order. File `src` paths are repository-relative and must exist;
programs default to mode `0o755`, files to `0o644`, and duplicate initfs paths name both sources.
`PROFILE=shell` makes x86-64 `make run` use QEMU's `-serial mon:stdio` interactive multiplexor;
type into the serial console and press `Ctrl-A x` to exit.

## Continuous checking

`sh scripts/ci-local.sh` runs `make check`, then `cargo xtask qtest --all` for each architecture
whose QEMU binary is installed, and writes a machine-readable summary to
`build/ci/summary.json`. It exits non-zero if any step failed.

The guard is part of every check. It scans tracked and untracked files for nightly-only toolchain
markers and rejects a `rust-toolchain.toml` channel that is not an exact stable version.
