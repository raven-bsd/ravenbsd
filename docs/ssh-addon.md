<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# SSH add-on: OpenSSH portable (K200)

Status: **BLOCKED on guest `/dev/null` + `socketpair` after the libc export fix**. T1 builds,
OpenSSH builds and links, and the libc C-ABI exports are in `libc.a`. The guest `ssh-version`
service still cannot run `sshd -V`: the minimal initfs has no `/dev/null`/service path namespace,
`socketpair` returns `EAFNOSUPPORT`, and `openat` returns `ENOENT`. No version line, qtest pass or
service/login claim is made. K195 owns the service/login integration; K196 owns fork/pty.

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

## Current blocker: guest `/dev/null` and `socketpair`

The rework fixed T1 and the libc C-ABI exports:

* `addons/tls/compat/tls_compat.c` now defines only the missing `futimes`; the T1 sysroot drops
  the broken `stdatomic.h` and installs the upstream public OpenSSL headers.
* `getifaddrs`, `freeifaddrs`, `getdtablesize`, `__res_state` and `res_query` are exported from
  `libc.a` through the `raven-libc-net`/`raven-libc-resolv` implementations, with host tests and
  `Symbols.list`/`Symbol.map` entries.
* `timeout 600 sh addons/ssh/build.sh` now links `ssh`, `sshd` and `ssh-keygen`.

The host kernel cannot execute the RavenBSD static binaries (`sshd -V`/`ssh -V` hang), so the
target is the guest qtest. The `ssh-version` initfs runs a thin wrapper that calls the real
upstream `sshd_main("-V")`; the guest reports:

```
ssh-version /dev/null: socketpair errno=97 openat errno=2
Couldn't open /dev/null: No such file or directory
svcd: ssh-version failed: exited before it became ready
init exited (1)
```

`socketpair` is `EAFNOSUPPORT` (97); `openat` of `/dev/null`/the staged `/bin/ssh-version` is
`ENOENT` (2) in the minimal service root. This is a devfs/service-namespace and socketpair
primitive gap, not an SSH protocol or crypto change. The `ssh-version` qtest is staged but red;
no `sshd -V`/`ssh -V` claim is made.

## Target gap

The `ssh-version` qtest, the `build/ssh/x86_64/bin/sshd`/`ssh` binaries and the test wrapper now
exist. The qtest is staged but red: the guest service fails at sshd's `/dev/null`/`socketpair`
probe with `socketpair errno=97` and `openat errno=2` (see above). This is the named runtime gap;
no `sshd -V`/`ssh -V` pass is claimed.

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
