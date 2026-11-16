<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# U2 - user utilities and NSS

SPDX-License-Identifier: BSD-3-Clause
Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors


Lane: kevin      Wave: 12      Milestone: M-0.2.0
Depends: U1, REL1b, REL1, U3 (for setuid su/passwd)      Blocks: U4
Estimated size: L   Type: userland
Status: draft (from U1)
Risk: medium

Owns-globs: userland/bin/useradd/** userland/bin/adduser/** userland/bin/passwd/** 
userland/bin/groupadd/** userland/bin/su/** userland/bin/id/** userland/bin/whoami/** 
crates/raven-userdb/** libc/ravenlibc/crates/raven-libc-nss/** 
libc/ravenlibc/crates/raven-libc/src/ffi/** libc/ravenlibc/crates/raven-libc-sys/** 
docs/user-management.md docs/man1/useradd.1 docs/man1/adduser.1 docs/man1/passwd.1 
docs/man1/groupadd.1 docs/man1/su.1 docs/man1/id.1 docs/man1/whoami.1 Cargo.toml Cargo.lock

## Why

U1 specifies the account files and tool behavior. This ticket implements the files-backed
`raven-userdb` library, the libc NSS-files surface and the seven 0.2.0 tools without changing the
0.1.0 root/`raven` login path.

## Contract

* Add `crates/raven-userdb` (`no_std` + alloc, `#![forbid(unsafe_code)]`, host tests in
  `src/*_ut.rs`): parse/render `/etc/passwd`, `/etc/group`, `/etc/shadow`; validate fields,
  uniqueness and uid/gid ranges; provide lookup/iteration and atomic edit APIs with a lock and
  temporary-file rename. It must never hash or verify a password.
* Add the libc NSS-files surface: `getpwnam/getpwuid/getpwent/setpwent/endpwent`,
  `getgrnam/getgrgid/getgrent/setgrent/endgrent`, `_r` variants and `getgroups`; read
  `file:/etc/passwd` and `file:/etc/group`; files backend only, no `nsswitch.conf` loading.
* Implement `useradd`, `adduser`, `passwd`, `groupadd`, `su`, `id` and `whoami` with the U1
  semantics. `passwd` and `su` use `raven-auth` (REL1b) and must be root-owned; the setuid bit is
  enabled only after U3 proves it. `useradd -m` copies `/etc/skel`, creates the same-name primary
  group when `-g` is absent and locks the shadow password.
* Every new binary needs a man page in `docs/man1/` and a host test in the crate's `*_ut.rs`.
* Do not edit `crates/raven-abi` (frozen contract), the kernel or REL1's login binary. If an ABI
  symbol is missing, stop and report it.
* Root/`raven` and the REL1/REL1c `/etc` files remain valid; the tools treat root as uid 0 and
  never rewrite the root record unless root runs `passwd root` explicitly.

## Tests and acceptance

```
cargo test -p raven-userdb
cargo nextest run -p useradd -p adduser -p passwd -p groupadd -p su -p id -p whoami
cargo xtask qtest --case user-tools --arch x86_64
make check-fast
```

`user-tools` may be the U4 fixture if U4 owns the QEMU case; in that case run the host commands and
the U4 case. The ticket is not DONE until a non-root account can log in, `id`/`whoami` agree, and
`passwd` changes the shadow hash.

## Report

`tickets/kevin/reports/U2.md` with the tool table, NSS symbol list, unsafe blocks (expected none),
and the host/QEMU evidence.
