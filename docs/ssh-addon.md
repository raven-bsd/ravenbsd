<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# SSH add-on: OpenSSH portable (K200)

Status: **DONE for the add-on build and `sshd -V` version smoke.** T1 builds, the libc C-ABI
exports are in `libc.a`, and OpenSSH builds `ssh`/`sshd`/`ssh-keygen`; the guest `ssh-version`
qtest prints the real upstream `OpenSSH_10.6p2` line. The real AF_UNIX `socketpair` remains
`EAFNOSUPPORT`; `sshd` privilege separation needs it, **K206** implements it and **K195** consumes
it. No service or login claim.

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
timeout 300 cargo xtask qtest --case ssh-version --arch x86_64  # guest sshd -V smoke
```

The host kernel cannot execute the RavenBSD static binaries, so `sshd -V`/`ssh -V` evidence comes
from the guest `ssh-version` qtest, which calls the real upstream `sshd_main("-V")`.

The script stages the RavenBSD libc clang sysroot from `cargo xtask libc-pack --arch x86_64`,
adds the T1 `libcrypto.a`/`libssl.a` and headers, and configures OpenSSH with
`--host=x86_64-unknown-ravenbsd --with-ssl-dir=build/tls/x86_64 --without-openssl-header-check`.
Build outputs are intentionally out of tree under `build/ssh/`.

The build removes the RavenBSD forwarding `usr/include/stdatomic.h` from its **build sysroot**
before compiling: the header only does `#include_next <stdatomic.h>` and shadows clang's own
implementation, so clang 14 cannot find the C11 atomics. This is a build-time workaround for a
libc header defect, not an OpenSSH patch; the libc owner should fix the forwarding header.

## Version smoke and AF_UNIX socketpair handoff

T1 and the libc C-ABI exports are in place:

* `addons/tls/compat/tls_compat.c` defines only the missing `futimes`; the T1 sysroot drops the
  broken `stdatomic.h` and installs the upstream public OpenSSL headers.
* `getifaddrs`, `freeifaddrs`, `getdtablesize`, `__res_state` and `res_query` are exported from
  `libc.a` through the `raven-libc-net`/`raven-libc-resolv` implementations, with host tests and
  `Symbols.list`/`Symbol.map` entries.
* `timeout 600 sh addons/ssh/build.sh` links `ssh`, `sshd` and `ssh-keygen`.

The `ssh-version` initfs runs a thin wrapper that calls the real upstream `sshd_main("-V")`. It
opens the staged `/dev/null` first; if the service namespace cannot see it, it uses a real test fd
(`eventfd`, then a duplicate of an inherited fd), never a synthesized version. Guest serial:

```
OpenSSH_10.6p2, OpenSSL 3.5.0 8 Apr 2025
svcd: ssh-version up
svcd: settled
init exited (0)
```

The host kernel cannot execute the RavenBSD static binaries (`sshd -V`/`ssh -V` hang), so this
guest qtest is the target evidence.

The real **AF_UNIX `socketpair` remains `EAFNOSUPPORT` (97)**. The version smoke does not need it,
but `sshd` privilege separation does. **K206** implements AF_UNIX socketpair; **K195** consumes it
for the service/login path. This ticket makes no service or login claim.

## Target gap

The `ssh-version` qtest is green. The remaining service/login path is K195, with K206's AF_UNIX
socketpair for privilege separation; neither is claimed here.

## Donor reconciliation (DR-52)

| Topic | Illumos | BSD | Linux | Raven decision |
|---|---|---|---|---|
| SSH source | no SSH implementation to copy | OpenSSH portable BSD-style, pinned upstream | OpenSSH reference | vendor unmodified; portability patches only |
| Crypto | OpenSSL add-on | OpenSSH links OpenSSL | same | T1 add-on; no custom crypto |
| Build boundary | add-on process | add-on process | add-on process | C outside the base scaffold; static link against ravenlibc/OpenSSL |

Donor revisions: OpenSSH `d449e662b6b8b8cb20e852d8084c269af1c5e8f1` (V_10_6_P2); T1 OpenSSL
`636dfadc70ce26f2473870570bfd9ec352806b1d` (3.5.0). No donor code enters the base scaffold.

## Evidence tags

* OBSERVED: the vendored version and provenance pin; the T1 and libc export fixes; the guest
  `OpenSSH_10.6p2, OpenSSL 3.5.0` line.
* DONE: the add-on build and `sshd -V` version smoke.
* HANDOFF: the real AF_UNIX `socketpair` is `EAFNOSUPPORT` for `sshd` privilege separation;
  K206 implements it, K195 consumes it. No service/login claim.
