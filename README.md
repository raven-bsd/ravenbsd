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

    make check        # once the build system lands (ticket K01)
    make image run    # boot in QEMU (milestone M1)
