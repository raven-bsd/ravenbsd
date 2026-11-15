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
* Target status: the host tests pass (97, `--locked`). The uutils workspace is separate, so it
  carries its own `libc-ravenbsd/` copy of the merged K19 `toolchain/vendor/libc` (the crate
  version is 0.2.191 to satisfy the uutils lock and `nix ^0.2.186`), and `uucore` depends on the
  merged `toolchain/raven-getrandom`; the root `.cargo/config.toml` supplies the getrandom custom
  backend. `cargo xtask build uutils --arch x86_64` now passes the former getrandom/libc 0.2.191
  failures and stops on the K19 ravenbsd module's remaining ABI gaps: `filetime` (`lutimes`,
  `UTIME_OMIT`), `rlimit` (`getrlimit`/`setrlimit`), `nix` (37 errors) and `rustix` (107 errors:
  termios constants, `siginfo_t::si_status`, `waitid`, `POSIX_FADV_*`, `ST_*`). That is the
  C64d/K26a follow-up list; the profile switch and the `base-tools` qtest wait for it. See the
  K25a report for the exact compiler output.
