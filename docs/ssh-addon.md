<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# SSH add-on: OpenSSH portable (K200)

Status: **BLOCKED on T1**. The upstream OpenSSH portable source is vendored unmodified, but the
T1 OpenSSL add-on does not build against the current libc, so `sshd`/`ssh` cannot be cross-built
or run yet. No SSH protocol, crypto or service code is implemented here; K195 owns the service
and login integration.

## Pinned source

| | |
|---|---|
| Source | OpenSSH portable |
| Tag | `V_10_6_P2` (peeled commit `d449e662b6b8b8cb20e852d8084c269af1c5e8f1`) |
| Version | `OpenSSH_10.6p2` (`addons/ssh/openssh/version.h`) |
| Licence | OpenSSH `LICENCE` (BSD-style; ISC-compatible) |
| Vendor path | `addons/ssh/openssh/` (unmodified except excluded test private keys) |
| Provenance | `PROVENANCE.toml` import `addons/ssh/openssh`; `.provenance-ignore` exempts the tree from the first-party SPDX rule |

The upstream `regress/` test tree (including 45 private-key fixtures) is excluded because GitHub
push protection rejects the private keys and the build does not use `regress`. No protocol,
cipher, KEX, key-format or crypto source is changed. `addons/ssh/build.sh --check` fails if a
private key or a wrong version/pin reappears.

## Build

```sh
timeout 60 sh addons/ssh/build.sh --check    # pinned source, provenance, tools
timeout 600 sh addons/ssh/build.sh           # T1 prerequisite, then configure/make
timeout 60 build/ssh/x86_64/bin/sshd -V      # target smoke, after a successful build
```

The script stages the RavenBSD libc clang sysroot from `cargo xtask libc-pack --arch x86_64`,
adds the T1 `libcrypto.a`/`libssl.a` and headers, and configures OpenSSH with
`--host=x86_64-unknown-ravenbsd --with-ssl-dir=build/tls/x86_64 --without-openssl-header-check`.
Build outputs are intentionally out of tree under `build/ssh/`.

The build removes the RavenBSD forwarding `usr/include/stdatomic.h` from its **build sysroot**
before compiling: the header only does `#include_next <stdatomic.h>` and shadows clang's own
implementation, so clang 14 cannot find the C11 atomics. This is a build-time workaround for a
libc header defect, not an OpenSSH patch; the libc owner should fix the forwarding header.

## Current blocker: T1 OpenSSL

Observed on `origin/master` `86625900`:

1. **T1's own build** (`timeout 600 sh addons/tls/build.sh`) compiles OpenSSL but fails at
   `crypto/lhash/*.c` with:

   ```
   /usr/lib/llvm-14/lib/clang/14.0.0/include/stdatomic.h:21:
   .../build/tls/x86_64/sysroot/usr/include/stdatomic.h:10:15:
   fatal error: 'stdatomic.h' file not found
   #include_next <stdatomic.h>
   ```

2. **With a scratch T1 copy that removes only that sysroot header**, the build gets through the
   compile and fails at the static link with duplicate symbols between the T1 compat shim and
   the current `libc.a`:

   ```
   rust-lld: error: duplicate symbol: chmod
   rust-lld: error: duplicate symbol: pipe
   rust-lld: error: duplicate symbol: select
   rust-lld: error: duplicate symbol: openlog
   rust-lld: error: duplicate symbol: syslog
   ...
   make[1]: *** [Makefile:14546: apps/openssl] Error 1
   ```

Both failures are in `addons/tls/**`/libc, outside K200's Owns. No OpenSSH `configure`, link,
`sshd -V`, guest qtest or service/login claim is made. `addons/ssh/build.sh --build` invokes the
T1 script when the artifacts are missing and preserves its exact failure.

## Target gap

The `ssh-version` qtest and `build/ssh/x86_64/bin/sshd` do not exist yet. `--qtest` refuses to
stage an initfs until a completed build exists and the qtest wrapper/service is written. This is
the named test gap; it is not claimed as covered.

## Donor reconciliation (DR-52)

| Topic | Illumos | BSD | Linux | Raven decision |
|---|---|---|---|---|
| SSH source | no SSH implementation to copy | OpenSSH portable BSD-style, pinned upstream | OpenSSH reference | vendor unmodified; portability patches only |
| Crypto | OpenSSL add-on | OpenSSH links OpenSSL | same | T1 add-on; no custom crypto |
| Build boundary | add-on process | add-on process | add-on process | C outside the base scaffold; static link against ravenlibc/OpenSSL |

Donor revisions: OpenSSH `d449e662b6b8b8cb20e852d8084c269af1c5e8f1` (V_10_6_P2); T1 OpenSSL
`636dfadc70ce26f2473870570bfd9ec352806b1d` (3.5.0). No donor code enters the base scaffold.

## Evidence tags

* OBSERVED: the vendored version and provenance pin; the two T1 build failures above.
* BLOCKED: OpenSSH configure/build, `sshd -V`, and the ssh-version qtest; owner T1/kevin plus the
  libc `stdatomic.h` owner.
* PROPOSED: the OpenSSH configure/make commands in `addons/ssh/build.sh`; they are not claimed to
  work until T1 links.
