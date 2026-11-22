<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# Per-process root and chroot (K223)

Status: **OBSERVED** for the kernel root, libc `chroot` and the `chroot` qtest on x86_64 QEMU.
Symlink resolution inside a rooted process is a named **K224/vfsd** follow-up, not claimed here.

## Root model

`Process` stores `root: Mutex<String>`: a normalized absolute path without a trailing slash.
`"/"` is the unrooted view. The field is set only by `PROC_CHROOT`; `PROC_FORK`,
`PROC_CREATE` and `PROC_EXEC` inherit it, and a new kernel process starts at `"/"`.

`PROC_CHROOT(path, len)` (`0x0D`, raw reviewer-owned constant):

- `euid == 0`; any other effective user gets `EPERM` before the path is touched.
- The path is read through the fault-safe user-copy layer, bounded by the scheme path limit.
- A bare path uses the `file:` scheme; an explicit `file:`/`vfs:` prefix is accepted.
- The target is resolved inside the **current** root with lexical `.`/`..` handling; `..`
  clamps at the root.
- The server opens the resolved path with `O_DIRECTORY`, so a missing target is `ENOENT` and
  a regular file is `ENOTDIR`; the handle is closed and the normalized path becomes the root.

## Open enforcement

`kernel/src/syscall/ipc.rs` rewrites every `file:`/`vfs:` open for a rooted process:

```text
client path "/inside"          -> root + "/inside"
client path "/../outside"      -> root + "/outside"   (clamped at root)
client path "/inside/../../x"  -> root + "/x"
client path "inside"           -> root + "/inside"    (kernel-side cwd is "/")
```

The request that reaches vfsd therefore never contains an absolute path outside the root, and
cannot use `..` to climb out. Unrooted processes keep the exact existing scheme behaviour.
`chroot("/")` in a rooted process is idempotent and keeps the current root.

## Inheritance

- `PROC_FORK`: the child copies `source.root`.
- `PROC_CREATE`: the empty child copies the current process root (or `/` for the kernel).
- `PROC_EXEC`: image replacement keeps the same `Process`, so the root survives.
- `chdir("/")` is a libc-side cwd update; the next open is rooted by the kernel.

## Libc

`libc/ravenlibc/crates/raven-libc/src/platform/chroot.rs` is a real wrapper over the safe
`raven_libc_sys::sys::proc_chroot` call; the old unconditional `EPERM` stub is gone. `chdir`,
`getcwd` and `open` are unchanged outside a rooted process.

## qtest transcript

`tests/qtest/cases/chroot.toml` uses the `libc-test` profile and a C fixture on the writable
`/tmp` ramfs:

```text
chroot: start
chroot: nonroot eperm=1
chroot: empty-root ok
chroot: chroot ok
chroot: cwd=/
chroot: inside=1 outside=0
chroot: dotdot=0
chroot: notdir=1
chroot-child: inside=1 outside=0
chroot: child ok
chroot: ok
init exited (0)
```

The empty-root case uses `/tmp/var/empty` because the `libc-test` initfs has no writable
`/var`; the kernel root is path-agnostic, so the release `/var/empty` used by OpenSSH follows
the same code path.

## Rooted symlink resolution (K224)

The S11 `OPEN` packet carries the root as a prefix of its path plus `root_len` (zero for an
unrooted process), and vfsd receives that length in `Ctx`. Rooted opens resolve every component
in vfsd:

- relative symlink targets are resolved from the link's parent under the root;
- absolute targets are interpreted relative to the root, not the underlying filesystem root;
- `..` pops but cannot climb above the root;
- dangling and outside targets return `ENOENT`; `O_NOFOLLOW` on a final link returns `ELOOP`;
- the symlink walk is bounded at 40 links (`ELOOP`).

Unrooted processes and rooted paths that do not touch a symlink preserve the existing path
behaviour. The kernel still prefixes the lexical root before `OPEN`, so the packet path is already
inside the root; vfsd uses the new root field to resolve links in that same boundary.

The `chroot` qtest now covers: unrooted inside symlink, rooted inside symlink, absolute target,
`../` escape, dangling target, and the same inside/escape pair after fork/exec:

```text
chroot: unrooted good=1
chroot: symlink inside=1 abs=1 escape=0 rel=0 dangling=0
chroot-child: inside=1 outside=0 symlink=1 escape=0
```

## Donor reconciliation

| Topic | Illumos | BSD | Linux | Raven decision |
|---|---|---|---|---|
| chroot/per-process root | chroot behaviour only; CDDL, no copy | FreeBSD/NetBSD chroot/path outcomes only | Linux chroot/path outcome only | first-party kernel root + scheme path enforcement; no donor code. |

No donor code was copied.
