<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# VFS path robustness (K478)

Status: **OBSERVED on base `c185d551`, x86-64, TCG `smp=4` and KVM `smp=2`.** The guest service
`tests/smp/vfs-fuzz` runs under svcd after vfsd/initfsd on the `core-servers-files` profile. It
issues malformed paths through the kernel `OPEN` path (libc and raw) and the vfsd `CALL` path
(`lstat`/`readlink`), then chroots to `/etc` and repeats the escape attempts. It fixes nothing:
**the chroot escape below is a P0 finding for the owning lane.**

## Contract rows (OBSERVED)

41 rows, 2 deviations (the two escape rows), 2 escapes.

| Group | Vectors | Observed |
|---|---|---|
| P01-P02 | `open("/etc/mounts")`, `open("/")` | fd (legal controls) |
| P03-P04 | empty path; no scheme (`noscheme`) | ENOENT; raw empty path EINVAL |
| P05-P06 | `fi:le:/etc/mounts`; `file:/etc:mounts` | ENOENT |
| P07 | non-UTF-8 `file:/\xff` (raw) | EINVAL |
| P08 | embedded NUL `file:/etc/mounts\0junk` (raw, length includes NUL) | **rejected** (ENOENT); no truncation to the valid file |
| P09 | length-only path with no NUL in the buffer (raw) | accepted (the ABI is length-based) |
| P10-P12 | 1, 2 and 16 components | ENOENT |
| P13-P17 | 255 components; component 255; component 4096; total 4095; total 4096 | ENAMETOOLONG/ENOENT; no crash |
| P18 | 257-byte full path (kernel scheme PATH_MAX=256) | ENAMETOOLONG |
| P19-P23 | `file:/../../etc/mounts`, `file:/./../...`, `file:/../..`, `initfs:../../..`, `initfs:/../../init` | EINVAL (rejected; root `/` has no outside) |
| C01-C03 | `lstat("/etc/mounts")`; `lstat("/../../etc/mounts")`; 300-byte path | success; success (clamped); ENAMETOOLONG |
| C04-C05 | `readlink` on a regular file and on `/init` | EROFS (documented variation) |
| L01 | `lstat("/etc/mounts")` before chroot | success; populates the libc cached `file:/` root |
| CH00 | `chroot("/etc")` (euid 0) | 0 |
| CH01-CH02 | `open("/mounts")`, `open("/../mounts")` inside | success; the clamped path is the mount table, not `/init` |
| CH03-CH04 | `open("/init")`, `open("/../init")` (fresh OPEN) | ENOENT (kernel root clamp works) |
| CH05-CH06 | raw `file:/init`, `file:/../init` (fresh OPEN) | ENOENT (kernel root clamp works) |
| **CH07-CH08** | **`lstat("/init")`, `lstat("/../init")` on the cached root** | **ESCAPE: resolves the real `/init` (mode 0100755, size 3238544)** |

The suite prints one `VFS <id> PASS|FAIL <detail>` line per row and a stable FNV-1a summary. The
findings are reported; the service prints `vfs-fuzz: alive` and exits 0 so the determinism pair can
run.

## P0 finding: cached-root CALL escapes a chroot

After `chroot("/etc")`, a user-visible `lstat("/init")` (and `/../init`) returns the real image
`/init` outside the chroot:

```text
vfs-fuzz: ESCAPE id=CH07 path=lstat(/init) resolved=lstat mode=0100755 size=3238544
vfs-fuzz: ESCAPE id=CH08 path=lstat(/../init) resolved=lstat mode=0100755 size=3238544
VFS CH07 FAIL cached-root lstat(/init) escaped the chroot
VFS CH08 FAIL cached-root lstat(/../init) escaped the chroot
```

The fresh `open`/raw `OPEN` paths are correctly refused (CH03-CH06), so the escape is specific to
the vfsd `CALL` path through the libc's cached root handle:

* `libc/ravenlibc/crates/raven-libc-platform/src/platform.rs:1035` `root_stat` sends
  `Call::Lstat { path }` on the handle from `:1628 root_handle()`, which caches `file:/` on first
  use. `libc/ravenlibc/crates/raven-libc/src/platform/chroot.rs` changes the kernel root but does
  not invalidate that cache.
* `userland/servers/vfsd/src/main.rs:1481` handles `Call::Lstat` through
  `forward_one(decoded.call, &base, ...)` with the `base`/root of the `Opened` file (opened before
  the chroot); the caller's current process root is not part of the CALL and is not applied.
* `kernel/src/ipc/router.rs` `call` forwards the opaque CALL payload; it carries no current root
  and cannot re-scope the path operation.

Expected: `lstat("/init")` after `chroot("/etc")` returns ENOENT (or resolves under `/etc`), never
the image's real `/init`. Proposed follow-up (PROPOSED, not fixed here): invalidate the libc cached
root on `chroot`/`fchdir`, and make the vfsd path CALL apply the caller's current root (or carry a
root generation in the CALL). Owner: libc/VFS lane; regression: this suite's CH07/CH08 rows.

## Determinism

* Seed `0x4b343738`; the vectors are fixed; the table hash covers the row id, result and detail.
* TCG: 41 rows, 2 deviations, 2 escapes, `hash=0x06517fb6f933c5fa`, two identical boots.
* KVM2: the same 41 rows and hash, two identical boots.
* The first TCG attempt after the final fixture edit hit the case's 120 s cap while the host was
  running several other qtests (the log stops after P23, before the first `lstat`); the retry with
  the same caps passed in 185.74 s. The timeout log is preserved and is host contention, not a
  product failure.

## Coverage gaps

* The image has no symlinks (`find rootfs -type l` is empty and the `core-servers-files` profile
  stages no symlink), so the symlink-loop row is a documented gap.
* The `lstat`/`readlink` libc API is C-string based; embedded NUL and non-UTF-8 are exercised
  through raw `OPEN` only. A raw vfsd CALL encoder for those bytes is a follow-up.
* `readlink` on a regular file returns **EROFS** from vfsd (allowed as a documented variation, not
  EINVAL as POSIX suggests for a non-symlink).
* The chroot rows require euid 0; the service runs as root. If a future profile runs it unprivileged,
  the rows become a coverage gap (the suite records `chroot` failure).
* The chroot escape uses the libc-cached root; a user-held `file:/` fd opened before chroot and a
  raw CALL is the same family but not separately encoded.

## Donor reconciliation

Platform path work; the table was filled before the first line. POSIX defines the contract; FreeBSD
and NetBSD are permissive donors read for behaviour; Illumos and Linux are outcome-only. No donor
code was copied.

| Topic | POSIX | FreeBSD (BSD-2) | NetBSD (BSD-2) | Raven decision |
|---|---|---|---|---|
| `..` clamping at the root | `path_resolution(7)`, `chroot(2)` | `sys/kern/vfs_lookup.c` `lookup` / `namei` | `sys/kern/vfs_lookup.c` `namei` | kernel `process::resolve_root_path` clamps; vfsd `normalize_under_root` guards symlinks |
| chroot path scope | `chroot(2)` | `sys/kern/vfs_syscalls.c` `kern_chroot` | `sys/kern/vfs_syscalls.c` `sys_chroot` | `PROC_CHROOT`; test CH00/CH07 |
| absolute path on an open description | `openat(2)`, `path_resolution(7)` | `sys/kern/vfs_syscalls.c` `kern_openat` | `sys/kern/vfs_syscalls.c` `sys_openat` | cached-root CALL must use the current root; **escape** |
| component/path limits | `NAME_MAX`, `PATH_MAX` | `sys/sys/param.h`, `vfs_lookup.c` | `sys/sys/param.h`, `vfs_lookup.c` | kernel scheme PATH_MAX=256 (P18); vfsd limits |
| non-UTF-8 / NUL bytes | byte paths, NUL terminates | `copyinstr`/`namei` | `namei` | kernel `user_string` UTF-8 check; raw OPEN for NUL |

## K478 report

The full report is `tickets/bob/reports/K478.md`: the path table, the P0 with file:line, the hash,
the gates, and the Evidence block K478-R01..R05.
