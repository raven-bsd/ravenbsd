<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# U3 - kernel identity, permissions and setuid

SPDX-License-Identifier: BSD-3-Clause
Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors


Lane: stuart      Wave: 12      Milestone: M-0.2.0
Depends: U1, P2a, REL1      Blocks: U2 (setuid su/passwd), U4
Estimated size: L   Type: kernel/security (STRONG)
Status: draft (from U1)
Risk: high
Model-tier: STRONG

Owns-globs: kernel/src/process/** kernel/src/syscall/** kernel/src/ipc/** crates/raven-ipc/** 
crates/raven-proto-fs/** userland/servers/vfsd/** userland/servers/ramfsd/** 
userland/servers/fsd-ufs/** crates/raven-vfs-core/** crates/raven-ramfs-core/** 
userland/rt/src/spawn.rs userland/rt/src/sys.rs tests/qtest/cases/setuid* 
tests/fixtures/setuid/** docs/setuid.md Cargo.toml Cargo.lock

## Why

The kernel has `ProcessIds` and `PROC_SETCRED_EX`, but no filesystem permission enforcement and no
setuid/setgid exec path. A 0.2.0 `su`/`passwd` cannot be safe until the caller's credentials reach
the filesystem servers and an executable's setuid bit is applied atomically with its image.

## Contract

* Add a versioned caller-credential context to scheme requests (`uid`, `euid`, `gid`, `egid` and a
  bounded group list) so `vfsd`/`ramfsd`/`fsd-ufs` can enforce owner/group/other `rwx` on open,
  lookup, create, unlink, rename and exec. The packet/proto change is in `crates/raven-ipc` and
  `crates/raven-proto-fs`; keep old packets decodable or bump the protocol version explicitly.
* Enforce permissions in the three filesystem servers and the VFS proxy: root bypasses read/write
  but still needs an execute bit on directories/executables; sticky-bit `/tmp` deletion is limited
  to the owner/root; `nosuid` mounts suppress setuid/setgid.
* Add a native exec/spawn path that takes an open file handle, obtains its `Stat` atomically,
  applies `S_ISUID`/`S_ISGID` to the new process's effective/saved ids, and starts the image. No
  setuid for scripts or non-ELF images; a setuid file without execute is an error.
* Keep `PROC_SETCRED_EX` semantics (root privileged, unprivileged allowed-id set, groups only for
  root); add tests for saved ids and the credential context. `crates/raven-abi` is frozen: use the
  existing raw constants or stop for a contract change.
* `umask` remains a libc/session value in 0.2.0; the servers use the create mode sent by the
  client. Document that kernel umask is deferred.
* No change to the 0.1.0 root login/REL2b gate; root remains uid 0 and can read/write as today.

## Tests and acceptance

```
cargo test -p raven-ipc -p raven-proto-fs -p raven-vfs-core -p raven-ramfs-core
cargo check -p raven-kernel --target x86_64-unknown-none
cargo check -p raven-kernel --target aarch64-unknown-none-softfloat
cargo xtask qtest --case setuid --arch x86_64
cargo xtask qtest --case setuid --arch aarch64
make check-fast
```

The `setuid` fixture must prove: a 0644 file is not executable; a 0755 non-setuid file keeps the
caller ids; a setuid-root 4755 file runs with euid 0; a setgid file gets the file group; a
setuid bit on a script is ignored; a `nosuid` mount suppresses the bit; a non-root caller cannot
read a 0600 root file through `file:`. No fake success if the platform cannot run the case.

## Report

`tickets/stuart/reports/U3.md` with the protocol change, the permission matrix, unsafe blocks, the
setuid transcript and the checkpoint checks.
