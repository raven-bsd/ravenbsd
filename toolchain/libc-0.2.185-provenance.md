<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# Vendored libc 0.2.185 provenance (K02)

This is the source/provenance record for `toolchain/patches/0002-libc-ravenbsd.patch`.
The root `PROVENANCE.toml` is lead-owned and remains unchanged; the reviewer integrates this
record into the root ledger before eventual acceptance.

## Registry release copied

| Field | Value |
|---|---|
| Crate | `libc` |
| Version | `0.2.185` |
| Source | crates.io registry cache (`libc-0.2.185.crate`) |
| Archive SHA-256 | `52ff2c0fe9bc6cb6b14a0592c2ff4fa9ceb83eea9db979b0487cd054946a2b8f` |
| `.cargo_vcs_info.json` git SHA | `71d5bfcc1bda05da1783666fc2cd7d9669c9c4c8` |
| Licence | `MIT OR Apache-2.0` |

The archive SHA-256 matches the `checksum` for `libc 0.2.185` in Rust 1.97.1's
`library/Cargo.lock` before the path patch. The `.cargo_vcs_info.json` SHA is the upstream libc
commit that the registry release was published from.

## Rust base tree

| Field | Value |
|---|---|
| Release | Rust `1.97.1` |
| Tag / commit | `1.97.1` / `8bab26f4f68e0e26f0bb7960be334d5b520ea452` |
| Sources | both patches are diffs against a pristine 1.97.1 source tree |
| Licence | `MIT OR Apache-2.0` |

## Donor checkout consulted, not copied

The `rust-libc` donor checkout in `ravenbsd-resources/rust-libc` is at revision
`7631ff2ae852bdef359933b65349773f58733ece` (`Cargo.toml` version `1.0.0-alpha.5`). It was read
for structure and the `redox` module shape, but it is **not** the source copied into the patch.
The copied source is the 0.2.185 registry release identified above. Do not use the donor revision
as the upstream revision of the copied release.

## Licence texts

The exact licence texts are carried in `toolchain/licenses/`:

| File | SHA-256 |
|---|---|
| `libc-0.2.185-LICENSE-MIT` | `123a331b5dbf04c30097fa43b8f858bc85df671fe776de498d01f3d6b7c1f69e` |
| `libc-0.2.185-LICENSE-APACHE` | `62c7a1e35f56406896d7aa7ca52d0cc0d272ac022b5d2796e7d6905db8a3636a` |
| `rust-1.97.1-LICENSE-MIT` | `b71bd43a069ca0641a9ecfe585ca7b3c53b5cc1608f8b68321168698e28b5ea1` |
| `rust-1.97.1-LICENSE-APACHE` | `62c7a1e35f56406896d7aa7ca52d0cc0d272ac022b5d2796e7d6905db8a3636a` |

The same `LICENSE-MIT` and `LICENSE-APACHE` files are also present inside the vendored
`library/libc-ravenbsd/` directory in the patch.

## Modified file groups

`diff -qr` between the vendored `library/libc-ravenbsd/src` and the cached registry
`libc-0.2.185/src` reports only:

* `src/unix/mod.rs` — adds the `target_os = "ravenbsd"` module selection and an empty
  native-link branch (ralibc is linked by the platform runtime).
* `src/unix/ravenbsd/mod.rs` — new file, copied from `src/unix/redox/mod.rs`; changes:
  `EWOULDBLOCK` is `EAGAIN` (11) instead of 41, `EDEADLOCK` is `EDEADLK` (35) instead of 58,
  and `FD_CLOEXEC` carries a `dead_code` allowance. The upstream `src/unix/redox/mod.rs` in the
  same vendored crate is unchanged.
* `Cargo.toml`, `build.rs` and every other `src/**` file are byte-identical to the cached
  0.2.185 sources.

The Rust workspace side (`library/Cargo.toml`, `library/Cargo.lock`) is modified to point the
`libc` patch at `library/libc-ravenbsd` and to drop the registry source/checksum for that path
dependency.

## Repeatable checks

```
sha256sum ~/.cargo/registry/cache/*/libc-0.2.185.crate
cat ~/.cargo/registry/src/*/libc-0.2.185/.cargo_vcs_info.json
diff -qr library/libc-ravenbsd/src ~/.cargo/registry/src/*/libc-0.2.185/src
python3 toolchain/audit-errno.py
python3 <reviewer>/k02-errno-audit.py toolchain/patches/0002-libc-ravenbsd.patch
```
