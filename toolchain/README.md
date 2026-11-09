<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# RavenBSD Rust toolchain patch set

Status: spike result (ticket K02), 2026-11-09.

This directory holds the patch series that adds `x86_64-unknown-ravenbsd` (and draft
`aarch64-unknown-ravenbsd` / `riscv64gc-unknown-ravenbsd`) to a released Rust source tree, plus the
bootstrap configuration and the build wrapper for `make toolchain` (ticket K19).

## Status (OBSERVED)

* `git apply --check` of both patches passes against a pristine Rust 1.97.1 tree.
* The real `build.sh` path was exercised manually on the release tarball: extract
  `rustc-1.97.1-src.tar.xz`, apply both patches with `patch -p1`, copy `bootstrap.toml`, then
  `BOOTSTRAP_SKIP_TARGET_SANITY=1 ./x.py check library --target x86_64-unknown-ravenbsd` exits 0.
* `BOOTSTRAP_SKIP_TARGET_SANITY=1 ./x.py build --stage 1 library/std --target
  x86_64-unknown-ravenbsd` exits 0 on the same tree.
* Tarball check wall time: 3:43 (20 logical CPUs, 31 GiB RAM), peak RSS ~1.40 GiB. The
  `library/std` build after the check: 25.7 s, peak RSS ~1.06 GiB. Source tree plus build
  directory on disk: ~7.5 GiB (the tarball itself is 232 MiB).
* A separate clean git-checkout run (`submodules = true`, which fetched a 2.3 GiB
  `src/llvm-project`) took 5:08. The release tarball already contains `library/backtrace` and
  `src/llvm-project`, so this directory's `bootstrap.toml` sets `submodules = false` and relies on
  `llvm.download-ci-llvm = true`.
* The target is registered in `rustc_target` (`Os::Ravenbsd`) and the stage1 compiler built from
  the patched source knows it. The stage0 compiler does not, so bootstrap's target sanity check
  must be skipped; `build.sh` sets `BOOTSTRAP_SKIP_TARGET_SANITY=1` for that reason.
* `bootstrap.toml` with `llvm.download-ci-llvm = true`, `rust.channel = "stable"`, the host and
  `x86_64-unknown-ravenbsd` targets, and `build.extended = true` is accepted by bootstrap. A full
  toolchain build (and therefore the actual LLVM download) was not run; the ticket forbids a full
  multi-hour build.

## Layout

| Path | What |
|---|---|
| `rust-version` | pinned release `1.97.1` and the SHA-256 of `rustc-1.97.1-src.tar.xz` |
| `bootstrap.toml` | bootstrap config: host + `x86_64-unknown-ravenbsd`, stable channel, CI LLVM, `build/toolchain` prefix |
| `patches/0001-add-ravenbsd-targets.patch` | target specs, `Os::Ravenbsd`, supported-target registration, std cfg changes |
| `patches/0002-libc-ravenbsd.patch` | vendored `libc` 0.2.185 with a `ravenbsd` unix module, plus `library/Cargo.toml`/`Cargo.lock` |
| `build.sh` | POSIX wrapper: download, verify, extract, patch, `x.py install`, `rustup toolchain link` |
| `ralibc-symbols.txt` | top libc symbols referenced by `library/std` (raw material for K20-K22) |

`build.sh --dry-run` prints the steps without network, extraction or installation.

## Target properties

* `os = "ravenbsd"`, `family = unix`, `env = relibc`.
* `dynamic_linking = false` (phase 1 is static), `static_position_independent_executables = true`,
  `panic_strategy = Abort`, `has_thread_local = true` (std's thread-local module needs TLS;
  ralibc must provide the block), `linker = "rust-lld"`.
* LLVM target triples are `x86_64-unknown-none`, `aarch64-unknown-none` and
  `riscv64-unknown-none-elf` because LLVM has no RavenBSD OS. This is a pragmatic choice for the
  draft; the object format is ELF in all three cases.

## Redox cfg sites (48 in Rust 1.97.1) and disposition

44 sites are mirrored by adding `target_os = "ravenbsd"` next to `target_os = "redox"`.
Four are deliberately different:

| File:lines | Disposition |
|---|---|
| `library/std/src/fs/tests.rs:1991,2258,2309,2379,2471` | mirrored (test gates) |
| `library/std/src/os/mod.rs:168` | mirrored: `os::redox` extension module is reused for RavenBSD |
| `library/std/src/os/unix/mod.rs:78` | mirrored |
| `library/std/src/os/unix/net/listener.rs:77,95` | mirrored |
| `library/std/src/sys/args/unix.rs:79` | mirrored |
| `library/std/src/sys/env_consts.rs:249` | **not mirrored as-is**: a separate `ravenbsd` module sets `OS = "ravenbsd"` |
| `library/std/src/sys/fd/unix.rs:561,585` | mirrored |
| `library/std/src/sys/fs/unix.rs:31,48,280,413,439,465,857,955,1016,1149,1205,1222,1794,1859,2145,2226,2514,2528` | mirrored |
| `library/std/src/sys/io/error/unix.rs:22` | mirrored |
| `library/std/src/sys/pal/unix/mod.rs:85` | mirrored |
| `library/std/src/sys/pal/unix/sync/condvar.rs:143,198` | mirrored |
| `library/std/src/sys/paths/unix.rs:345` | **not mirrored**: RavenBSD uses the `/proc/self/exe` branch, not `/scheme/sys/exe` |
| `library/std/src/sys/paths/unix.rs:426,440` | mirrored (`home_dir` fallback) |
| `library/std/src/sys/pipe/unix.rs:25` | mirrored |
| `library/std/src/sys/process/unix/unix.rs:299,313` | mirrored (`setgroups` not available yet) |
| `library/std/src/sys/random/mod.rs:61` | **not mirrored**: uses `unix_legacy` (`/dev/urandom`), not `/scheme/rand` |
| `library/std/src/sys/thread/mod.rs:58,84` | mirrored |
| `library/std/src/sys/thread/unix.rs:5` | mirrored |
| `library/std/src/sys/thread_local/mod.rs:64` | mirrored |
| `library/std/tests/thread.rs:83` | mirrored |
| `library/unwind/src/lib.rs:154` | **not mirrored**: `panic=abort`; no `gcc_eh` link |

Additions outside the 48 Redox sites:

* `library/std/build.rs`: mark `ravenbsd` as a platform that does not need
  `restricted_std` (otherwise every std consumer would need an unstable feature).
* `library/std/src/sys/paths/unix.rs`: add `ravenbsd` to the `/proc/self/exe` branch.
* `library/libc-ravenbsd`: libc 0.2.185 with `src/unix/ravenbsd/mod.rs` copied from the
  `redox` module and selected in `src/unix/mod.rs`; its link branch has no native `c`
  dependency. The `FD_CLOEXEC` constant carries a `dead_code` allowance because std does
  not use it yet.
* The errno constants in the copied module are Redox's; a differential audit against Linux's
  values (DR-13) is required.

## Known unknowns

* Only `x86_64-unknown-ravenbsd` was checked end to end. The aarch64/riscv64 target specs are
  draft and untested.
* No executable was linked: `x.py check`/`build library/std` compile the library, but linking
  against ralibc and the userland linker script is K19/K06 work.
* `std::thread`, `std::fs`, `std::process`, `std::net` and TLS will need the ralibc symbols and
  runtime support listed in `ralibc-symbols.txt`; dynamic loading is deferred (`dynamic_linking
  = false`).
* `llvm.download-ci-llvm = true` was accepted by bootstrap but the actual download was not
  exercised; a full toolchain build is required to confirm it.
* The vendored libc is the exact 0.2.185 source from crates.io (matching `library/Cargo.lock`),
  not the donor `rust-libc` 1.0.0-alpha.5 checkout; the PROVENANCE entry records both.

## Estimate for K19

Observed spike work is one minion session; the remaining K19 work is the full toolchain
build/install path and CI: roughly **12-20 hours of minion time** (UNVERIFIED), dominated by
the first full stage1/stage2 build and debugging the download-ci-llvm path, not by the patch
itself. ralibc implementation work for std is separate (K20-K22).
