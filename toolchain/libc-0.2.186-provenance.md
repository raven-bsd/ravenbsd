<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# Vendored libc 0.2.186 provenance (K19a/K26a)

`toolchain/vendor/libc` is the crates.io `libc 0.2.186` release with the K19
ravenbsd unix module grafted in. 0.2.185 was the K19 toolchain patch version,
but `nix 0.31.3` (brush) requires `libc ^0.2.186`, so the shared patch is one
release newer. The ravenbsd module itself is the K19 0.2.185 module; the
0.2.186 macros are compatible with it.

## Registry release

| Field | Value |
|---|---|
| Crate | `libc` |
| Version | `0.2.186` |
| Source | crates.io registry cache (`libc-0.2.186.crate`) |
| Archive SHA-256 | `68ab91017fe16c622486840e4c83c9a37afeff978bd239b5293d61ece587de66` |
| `.cargo_vcs_info.json` git SHA | `42620ffc4109dc32e02f1cae9e63a3f4311b4b71` |
| Licence | `MIT OR Apache-2.0` |

## Ravenbsd changes

* `src/unix/mod.rs` adds the `target_os = "ravenbsd"` link branch (the platform
  runtime links ralibc, so the crate declares `extern "C" {}`) and selects the
  new module.
* `src/unix/ravenbsd/mod.rs` is the K19 module from
  `toolchain/patches/0002-libc-ravenbsd.patch` (the 0.2.185 source), copied
  unchanged.

Every other `src/**`, `build.rs` and `Cargo.toml` byte is the 0.2.186 release.
MIT SPDX/copyright headers were added so the repository provenance checker can
identify the files; the licence expression is unchanged. `LICENSE-APACHE`
carries a short provenance notice above the unmodified licence text. The
registry-only metadata and development files (`.cargo-ok`,
`.cargo_vcs_info.json`, dotfiles, `Cargo.lock`, `Cargo.toml.orig`, `tests/`)
were omitted as in the 0.2.185 vendor.

## Use

The root `Cargo.toml` has `[patch.crates-io] libc = { path = "toolchain/vendor/libc" }`;
the crate is excluded from the root workspace. The root lock, the brush
workspace lock and the K19a getrandom probe all pin 0.2.186.
