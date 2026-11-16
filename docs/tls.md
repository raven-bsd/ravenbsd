<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# TLS add-on: OpenSSL 3.5

Status: T1 implementation. The upstream OpenSSL 3.5.0 source is vendored unmodified and cross-built
as a C add-on; the base scaffold remains C-free. Target evidence is the `openssl-version` qtest.

## Decision

DR-45: RavenBSD never implements TLS in-house. BSD systems use OpenSSL, and RavenBSD uses the same
C code as an **add-on** outside the Rust, C-free base scaffold. Only portability patches are
allowed; crypto primitives are never modified.

## Pinned source

| | |
|---|---|
| Source | OpenSSL |
| Version | 3.5.0 (8 Apr 2025) |
| Upstream commit | `636dfadc70ce26f2473870570bfd9ec352806b1d` |
| Licence | Apache-2.0 (`addons/tls/openssl/LICENSE.txt`) |
| Vendor path | `addons/tls/openssl/` (unmodified) |
| Provenance | `PROVENANCE.toml` import `addons/tls/openssl`; the tree is exempt from the first-party SPDX rule in `.provenance-ignore` |

The source is vendored from the donor checkout recorded in `ravenbsd-resources/donors.lock`. The
build reads it out of tree; no generated file is written into the vendored tree. The upstream
test/demo/fuzz private-key fixtures are excluded: GitHub push protection rejects them and the
add-on is built with `no-tests`, so they are not needed. The crypto, build and public API sources
are otherwise unmodified; `addons/tls/build.sh --check` fails if a private key reappears.

## Build

```sh
sh addons/tls/build.sh --check   # pinned version, provenance, host tools; no build
sh addons/tls/build.sh           # build libcrypto, libssl and the openssl CLI
sh addons/tls/build.sh --qtest   # build and pack the openssl-version qtest initfs
```

Output lives under `build/tls/x86_64/`:

```
build/tls/x86_64/bin/openssl
build/tls/x86_64/bin/openssl-version
build/tls/x86_64/lib/libcrypto.a
build/tls/x86_64/lib/libssl.a
build/tls/x86_64/include/openssl/*.h
build/tls/x86_64/openssl-version-initfs.img
```

The script stages the RavenBSD libc clang sysroot from `cargo xtask libc-pack --arch x86_64`:
the public headers, the architecture and ABI-generated `bits/`, `libc.a`, the crt objects, the
`libdl`/`libpthread`/`librt` stubs and `libclang_rt.builtins.a`. It compiles with:

```
clang --target=x86_64-unknown-ravenbsd --sysroot=build/tls/x86_64/sysroot
```

The static link uses `tests/libc/target/x86_64.ld`, `ld.lld`, the RavenBSD `libc.a` and the
compiler-rt builtins. The OpenSSL configuration is:

```
linux-x86_64 no-shared no-tests no-asm no-dso no-engine no-async no-threads
no-legacy no-ssl3 no-comp
```

`no-asm`, `no-dso` and `no-shared` are the portability baseline: no architecture assembly, no
dynamic engine loading and static add-on libraries. `no-threads` is a first-port limitation; the
T2 TLS client must decide whether it needs the pthread build.

## Add-on boundary

OpenSSL is **not** linked into the kernel, loader, system servers, base utilities or the regular
`make image` target. The add-on is installed under the project's add-on prefix
(`/addons` on a target image) and consumers link `libcrypto.a`/`libssl.a` or run
`bin/openssl`. The C-free scaffold rule is unchanged; the add-on is a separate C process/library
set and keeps its Apache-2.0 licence.

## Compatibility shim

`addons/tls/compat/tls_compat.c` is a small BSD-3-Clause portability shim, not crypto code. The
RavenBSD libc does not yet export every POSIX call that OpenSSL's optional CLI paths reference:

* `chmod` uses `open` + `fchmod`; `readlink` and `tcgetattr`/`tcsetattr` come from libc.
* `mlock`/`munlock`/`madvise`/`msync` are no-ops (the add-on is a normal process, not a locked
  memory service).
* `openlog`/`syslog`/`closelog` write to stderr.
* `symlink`, `link`, `chown`, `fchown`, `utimes`, `futimes`, `pipe`, `select`, `alarm`, `atof` and
  `setbuf` are stubs where the libc has no wrapper; the upstream CLI paths that call them are not
  used by `openssl version`. **T2 must replace the `select`/`pipe` stubs or add the libc calls
  before relying on interactive or multi-connection client paths.**

No OpenSSL source file is modified. If a future libc adds these calls, the shim entries can be
removed without touching upstream.

## Target smoke

```sh
cargo xtask qtest --case openssl-version --arch x86_64
```

The case boots the normal base kernel/loader with a prepared initfs that contains a one-shot
service. `addons/tls/qtest/openssl_version_main.c` calls the upstream `version_main` entry point,
and routes its `printf` to the RavenBSD debug console because a freshly spawned C process has no
POSIX stdout fd yet; the installed `bin/openssl` remains the unmodified upstream application. The
build script's `--qtest` mode packs that initfs. The test runs under QEMU; no hardware claim is
made.

## CVE and update procedure

OpenSSL security advisories are tracked; this add-on may not ship a release with an open
high-severity advisory. To update:

1. Check out the chosen upstream 3.5.x/3.x tag from `ravenbsd-resources` (or fetch the tag in the
   donor checkout; the build itself never downloads).
2. Replace `addons/tls/openssl/` with the unmodified tree and update the `PROVENANCE.toml` import
   (`upstream_rev`, `copied`, notes) and this table.
3. Run `sh addons/tls/build.sh --check`, the build, and the `openssl-version` qtest. Review any new
   compatibility-shim symbols; never patch crypto primitives.
4. Record the advisory/version in the ticket or report and keep the Apache-2.0 notice with the
   source.

The add-on is a separate C component; it does not change the base image, the kernel ABI or the
Rust toolchain.
