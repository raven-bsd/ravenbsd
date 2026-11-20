<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# SSH add-on: OpenSSH portable (K200)

Status: **BLOCKED on libc exports after the T1 fix**. T1 OpenSSL now builds and OpenSSH
configures and compiles; the final static link is missing libc C-ABI exports. No `sshd -V`,
`ssh -V`, guest qtest or service/login claim is made. K195 owns the service/login integration;
K196 owns fork/pty.

## Pinned source

| | |
|---|---|
| Source | OpenSSH portable |
| Tag | `V_10_6_P2` (peeled commit `d449e662b6b8b8cb20e852d8084c269af1c5e8f1`) |
| Version | `OpenSSH_10.6p2` (`addons/ssh/openssh/version.h`) |
| Licence | OpenSSH `LICENCE` (BSD-style; ISC-compatible) |
| Vendor path | `addons/ssh/openssh/` (unmodified except the excluded `regress/` test tree) |
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

## Current blocker: libc exports after the T1 fix

The rework fixed T1: `addons/tls/compat/tls_compat.c` now defines only the missing `futimes`
(all other calls are exported by ravenlibc), the T1 build sysroot drops the broken forwarding
`stdatomic.h`, and the T1 install includes the upstream public headers (`openssl/macros.h`).
`timeout 600 sh addons/tls/build.sh` produces `libcrypto.a`/`libssl.a`.

OpenSSH then configures (with `--host=x86_64-unknown-linux-gnu` for config.sub feature selection,
`--without-zlib`, T1 staged without the cross `openssl` binary) and compiles its objects. The
static link of `ssh`/`sshd` is blocked by libc exports that the current `libc.a` does not provide:

```
undefined symbol: getifaddrs
undefined symbol: freeifaddrs
undefined symbol: getdtablesize
undefined symbol: __res_state
undefined symbol: res_query
```

`libc/ravenlibc/include` declares `getifaddrs`/`freeifaddrs` and `res_query`, and
`libc/ravenlibc/crates/raven-libc-net/src/iface.rs` and `raven-libc-resolv/src/compat.rs` contain
implementations, but the C-ABI wrappers are not in the packed `libc.a`; the `libs*` stubs are
empty. `getdtablesize` is a small fallback that `openbsd-compat/bsd-closefrom.c` can use via
`sysconf`. These are libc/K196 owners, not SSH protocol patches. No `sshd` binary, `-V`, or qtest
is produced until they are exported.

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
