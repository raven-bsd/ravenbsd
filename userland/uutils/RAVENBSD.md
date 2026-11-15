# RavenBSD uutils coreutils vendor

<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

This directory is the pinned uutils coreutils source, vendored so the build never reads the
read-only `ravenbsd-resources` checkout.

* Upstream: `uutils-coreutils` at `cf030b82ef519ab60ccacf763f7f086dd612d9c3` (0.13.0), MIT.
* Provenance: the single `[[import]]` row `userland/uutils` in `PROVENANCE.toml`; the tree is in
  `.provenance-ignore` because every upstream file already carries the upstream MIT notice in its
  own form and adding the RavenBSD SPDX line would mislicense it.
* Local patch: `src/uu/mv/src/mv.rs` only. The pinned commit is one commit before the upstream
  `cc1a8b2` "fix mv build" change; the tree calls `fsxattr::copy_xattrs_fd_ignore_unsupported`,
  which does not exist, and passes one argument to `show_xattr_failures` (which takes two). The
  two-line change uses the existing `copy_xattrs_fd` and `Some(to)`, exactly the upstream fix.
  Without it `cargo test -p uu_mv` does not compile.
* Selection: `xtask/src/uutils.rs` owns the first selected set
  (`ls cat cp mv rm ln mkdir rmdir touch stat df du echo pwd basename dirname true false sleep date
  head tail wc sort cut tr env id chmod chown`). The rest of the 109 crates stay vendored but are
  not built into the image until a later ticket adds them with tests and a man page.
* Target status: the host tests pass (97, `--locked`) and `cargo xtask build uutils --arch x86_64`
  links the 30 selected utilities for `x86_64-unknown-ravenbsd` into `build/x86_64/uutils/`. The
  workspace carries `libc-ravenbsd/` (the merged K19 libc module, crate version 0.2.191) and three
  cfg-arm compatibility copies, `nix-ravenbsd/`, `rustix-ravenbsd/` and `rlimit-ravenbsd/`; no
  algorithm changes. `uucore` links the merged `toolchain/raven-getrandom` backend.
* Link wiring: the Raven std target does not add `-lc`, so `xtask/src/uutils.rs` passes the staged
  `build/<arch>/libc.a`, `rcrt1.o`, `crti.o`, `crtn.o` and `userland/uutils/<arch>.ld`. The latter
  is the generic user layout plus `.tdata`/`.tbss` (`PT_TLS`) and the constructor arrays a std
  binary carries. `-C strip=symbols` keeps the staging tree at 91 MiB for 30 names.
* Remaining caveat: the installed `ravenbsd` std was built from the K19 libc module, whose fcntl,
  termios and clock constants predate the Raven ABI tables. Rebuilding the toolchain from the
  corrected `toolchain/vendor/libc` is a K19/C60 follow-up outside this ticket; the uutils binaries
  themselves use the corrected module.
