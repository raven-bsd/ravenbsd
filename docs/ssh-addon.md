<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# SSH add-on: OpenSSH portable (K200)

Status: **DONE for the add-on build, split server binaries and `sshd -V` version smoke.** T1
builds, the libc C-ABI exports are in `libc.a`, and OpenSSH builds `ssh`, `sshd`,
`sshd-session`, `sshd-auth` and `ssh-keygen`; the guest `ssh-version` qtest prints the real
upstream `OpenSSH_10.6p2` line. The AF_UNIX pair and control path are implemented by
K206/K210/K211; **K195** owns the service/login integration. No service or login claim.

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
`--host=x86_64-unknown-linux-gnu --libexecdir=/usr/libexec --with-ssl-dir=build/tls/x86_64
--without-openssl-header-check`. Build outputs are intentionally out of tree under `build/ssh/`.

### Split server binaries (OpenSSH 10.6, K212)

OpenSSH 10.6 runs the listener as three programs. `sshd` accepts the connection and re-execs
`sshd-session` (the unprivileged session) and `sshd-auth` (the authentication helper). Building
only `sshd` leaves the parent with no child to exec, so a connection resets at
`kex_exchange_identification`.

The build now produces and stages:

```text
build/ssh/x86_64/bin/ssh
build/ssh/x86_64/bin/sshd
build/ssh/x86_64/bin/ssh-keygen
build/ssh/x86_64/libexec/sshd-session
build/ssh/x86_64/libexec/sshd-auth
```

`--libexecdir=/usr/libexec` makes the compiled `_PATH_SSHD_SESSION` and `_PATH_SSHD_AUTH` target
paths, not `build/ssh/...` prefixes. The build fails if the strings are missing:

```text
timeout 60 strings build/ssh/x86_64/bin/sshd | grep /usr/libexec/sshd-session
timeout 60 strings build/ssh/x86_64/bin/sshd | grep /usr/libexec/sshd-auth
```

**K195 staging handoff:** install the two split binaries into the target root at the paths the
listener embeds:

```text
build/ssh/x86_64/libexec/sshd-session -> /usr/libexec/sshd-session
build/ssh/x86_64/libexec/sshd-auth    -> /usr/libexec/sshd-auth
```

The parent `sshd` must be staged with those paths present; the service/login manifest, host keys
and the real connection are K195's, not this add-on's.

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
* `timeout 600 sh addons/ssh/build.sh` links `ssh`, `sshd`, `sshd-session`, `sshd-auth` and
  `ssh-keygen`; the split binaries are staged under `build/ssh/x86_64/libexec/`.

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

The real AF_UNIX pair is now implemented: **K206** supplied the full-duplex `socketpair`,
**K210** added kernel control messages and peer credentials, and **K211** mapped `SCM_RIGHTS`,
`SO_PEERCRED` and `getpeereid` through libc. The version smoke does not need them, but `sshd`
privilege separation does. **K195** consumes the pair and the split binaries for the
service/login path.

## Target gap

The `ssh-version` qtest is green and the split server binaries are staged. The remaining
service/login work belongs to K195: the profile/service manifest, host-key generation into a
writable `/etc/ssh`, `/usr/libexec` staging of `sshd-session`/`sshd-auth`, and the real login
qtest. No service or login is claimed here.

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
* DONE: the add-on build, the split `sshd-session`/`sshd-auth` binaries and the `sshd -V` version
  smoke.
* HANDOFF: K195 stages `build/ssh/x86_64/libexec/sshd-session` and `sshd-auth` at
  `/usr/libexec/`, adds the service/login profile and host keys, and runs the real login qtest.
  No service/login claim here.
