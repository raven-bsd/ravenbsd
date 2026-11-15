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
* Target status: the host tests pass. The Raven target build is blocked before the link on the
  external `libc` crate: crates.io `libc 0.2.191` has no `target_os = "ravenbsd"` module. The
  K19 patched `libc 0.2.185` supplies one for std, but `rustix 1.1.6` (a uutils dependency) needs
  termios constants (`ECHOKE`, `FLUSHO`, `PENDIN`, `EXTPROC`) and `siginfo_t::si_status` that the
  K19 module does not define. That is a C64d/libc-ABI gap, not a uutils change; the profile
  switch and the `base-tools` qtest wait for it. See the K25a report for the exact compiler output.
