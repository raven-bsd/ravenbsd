<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# Build artifacts and their stamps (K441)

Status: **the static `libc.a` and the dynamic `libc.so.1`/`ld-elf.so.1` are stamped with the libc
source hash; `libc-pack --verify` and `libc-link --verify` fail closed with
`artifact stale: run cargo xtask libc-pack` (or `libc-link`). `scripts/build.sh` verifies an existing
stamp before it builds, the in-scope qtest fixture readers that consume the sysroot verify it before
they link, and the stale-artifact regression is
`tests/release/workspace-build/libc-stamp.sh`.** The remaining direct reader (`tests/libc/target/build.sh`,
plus the Rust emptiness test) is a follow-up outside K441's Owns.

## The defect

Two measured false results came from trusting `build/` without a freshness check:

* **False green (K438):** a mutation set `count=0` in `unix_recvmsg`, but the `fdpass` qtest passed
  because the image carried a stale `libc.so.1`.
* **False red (MPTCP2c2):** `make check-fast` on a tree with no product diff failed
  `13053/13054` (`server_archive_exports_all_fifty_names`, `missing port_create`) because
  `build/x86_64/libc.a` was a stale tiny-set archive.

Stale files under `build/` are not source state; no test may silently pass or fail against an
artifact older than the sources.

## Stamp

After a successful `libc-pack`, `build/<arch>/libc.stamp` contains:

```text
format=k441.1
arch=x86_64
set=server
sources=<FNV-1a of sorted libc/ravenlibc paths+contents, Cargo.lock, rust-toolchain.toml>
libc_a=<hash of build/<arch>/libc.a>
sysroot_libc_a=<hash of build/<arch>/sysroot/usr/lib/libc.a>
```

After a successful `libc-link`, `build/<arch>/libc-dso.stamp` adds the same source hash plus the
hashes of `libc.so.1`, the sysroot copy, `ld-elf.so.1` and its sysroot copy.

The source hash is over **contents**, not mtimes; an editor save or `touch` without a rebuild
changes it only when the bytes change. The stamp is written atomically (`.tmp` then rename), so a
failed build never publishes a stamp for a partial archive.

## Commands

```text
cargo xtask libc-pack --arch x86_64              # build, pack, stage the sysroot, write the stamp
cargo xtask libc-pack --arch x86_64 --verify     # freshness only; no build
cargo xtask libc-link --arch x86_64              # build the DSO, stage it, write the DSO stamp
cargo xtask libc-link --arch x86_64 --verify     # freshness only
```

`--verify` exits 0 and prints `libc-pack: verify: ok` (or `libc-link: verify: ok`) when the stamp
matches the tree and the artifact hashes. Otherwise it prints exactly:

```text
artifact stale: run cargo xtask libc-pack
```

and exits 1. `--verify` never rebuilds; the caller either runs the documented build command or
stops.

## Consumers

* `scripts/build.sh` checks an **existing** `libc.stamp` / `libc-dso.stamp` before building and
  stops with the documented message on a mismatch. A clean clone has no stamp and skips the check;
  the Rust userland build does not consume libc.
* The qtest fixture readers that link the static sysroot directly
  (`tests/qtest/cases/socketpair-fixture/build.sh`, `tests/qtest/cases/itimer-fixture/build.sh`) run
  `libc-pack --verify` (and `libc-link --verify` when a DSO stamp exists) before linking. A clean
  clone with no stamp skips the check and uses the existing build/refresh path.
* `libc_pack::run` is the producer: the image/initfs path calls it and therefore always stages a
  fresh archive. `libc-link` is the DSO producer.
* `tests/release/workspace-build/libc-stamp.sh` is the regression: it verifies a fresh artifact,
  edits a libc source without rebuilding and requires the stale error from `--verify` **and** from
  both fixture readers, then reproduces the tiny-set false red (`--set tiny` followed by a server
  `--verify`), rebuilds, and verifies green.

## Regression

`sh tests/release/workspace-build/libc-stamp.sh` (foreground; the script gives each stage its own
timeout). It uses a temporary copy of the edited source and restores it on every exit path, so a
failed assertion does not leave the tree modified.

## Open gaps

* The Rust `server_archive_exports_all_fifty_names` test and `tests/libc/target/build.sh` still read
  `build/<arch>/libc.a` or the sysroot directly; adding `--verify` there is outside K441's Owns and
  is the next step. The `tests/qtest/cases/*-fixture/` readers that consume the sysroot now verify.
  `archive_exports` itself builds its own archive (K437) and is unaffected.
* The source hash is FNV-1a over sorted paths/contents for a cheap local check, not a
  cryptographic provenance hash; it is not a release signature.

## Donor reconciliation

Build tooling and tests only; no platform expression is copied. FreeBSD `share/mk`/`sys/conf` and
NetBSD `share/mk` were not needed; there is no donor code path. The source-hash/stamp design is
first-party.
