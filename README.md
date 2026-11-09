# RavenBSD

A modern BSD: small, portable, coherent. A Rust microkernel (ours, Redox-derived), Rust system servers, a
Rust libc and base utilities, built with `cargo` driven by a `Makefile`, producing an image that boots.
You type `uname -a` and see `RavenBSD`.

- Language: Rust on **stable** only (pinned in `rust-toolchain.toml`). No C in the scaffold.
- Licence: BSD-3-Clause (`LICENSE`). Third-party code: `PROVENANCE.toml`, `NOTICE`.
- AI-assisted contributions are welcome: `AI_POLICY.md`.
- Status: pre-M1. Contract crates exist (`crates/raven-abi`, `crates/raven-bootinfo`); the kernel is being written.

Planning, architecture and tickets live in the separate private `ravenbsd-prjmgmt` repository; donor
reference sources live in `ravenbsd-resources` and are never vendored here.

Quick start (Ubuntu/Debian or Fedora):

    make setup      # install packages, Rust toolchain, firmware paths; verifies everything
    make dev        # debug build of everything that exists (ARCH=x86_64|aarch64|riscv64)
    make release    # smallest optimised build
    make check      # stable guard, fmt, clippy, tests
    make clean      # remove build output and start over
    make doctor     # what is installed / missing

`SETUP_ARGS="--dry-run" make setup` prints what would be installed. Scripts live in `scripts/`; the Makefile only calls them.
