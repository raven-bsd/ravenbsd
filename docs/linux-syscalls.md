<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# linuxd syscall batch: files (B23)

Status: B23, 2026-11-16. Evidence tags: **OBSERVED** (run on the target or read in the source),
**PROPOSED** (not implemented), **UNVERIFIED** (not exercised).

## 1. Scope

B23 serves the Linux file I/O surface against the `file:` namespace. A Linux absolute path is
opened as `file:<path>`; an explicit `file:` or `ufs:` path passes through unchanged. The
synthetic `/proc` and `/sys` files keep their in-memory implementation and remain read-only.

| nr (x86-64) | name | behaviour |
|---|---|---|
| 0 | `read` | remote descriptor or synthetic regular file; copy-out to the user buffer |
| 1 | `write` | stdout/stderr console, or a remote descriptor |
| 2 | `open` | legacy alias for `openat(AT_FDCWD, path, flags)` |
| 3 | `close` | releases a remote scheme handle and the descriptor slot |
| 8 | `lseek` | remote `SEEK` on the scheme OFD, or the synthetic cursor |
| 17 | `pread64` | remote positional read; the cursor does not move |
| 18 | `pwrite64` | remote positional write; the cursor does not move |
| 72 | `fcntl` | `F_DUPFD`, `F_DUPFD_CLOEXEC`, `F_GETFD`, `F_SETFD`, `F_GETFL`, `F_SETFL` |
| 257 | `openat` | `AT_FDCWD`/absolute paths, `file:`/`ufs:` scheme paths, synthetic proc/sys |
| 267 | `readlinkat` | existing synthetic symlink target (path argument fixed to 1) |
| 292 | `dup3` | duplicate a descriptor at an exact fd with `O_CLOEXEC` |

`openat` accepts the usual access, create, truncate, append, directory, no-follow and close-on-exec
flags; `raven_rt::fs::s11_flags` maps them to the S11 wire flags and rejects unsupported bits.
`close` now releases a remote file server handle; `dup3`/`F_DUPFD` duplicate the scheme handle so
the server's open-file description and cursor are shared.

## 2. Descriptors and path resolution

`LinuxProcess` stores three standard descriptors plus a dynamic table starting at fd 3. A remote
descriptor is `FileKind::Remote(handle)`; a synthetic one is the existing regular/directory/symlink
content. `fd_flags` carries `FD_CLOEXEC`, and `open` flags are returned by `F_GETFL`.

Relative `openat` paths resolve against the cwd (`/` by default) or the opened directory's path;
`.` and `..` are normalised without leaving `/`, and paths longer than 128 bytes are
`ENAMETOOLONG`. A scheme path contains a `:` before its first `/` and is not normalised.

The dispatcher remains a pure function table: `files.rs` resolves the path and queues a `FileOp`.
`main.rs` performs the kernel `OPEN`/`READ`/`WRITE`/`SEEK`/`PREAD`/`PWRITE`/`DUP`/`CLOSE` calls,
copies data to/from the Linux address space, and installs the resulting descriptor. This keeps
every kernel operation in the trap loop where the process handles and memory objects live.

## 3. qtest setup

`tests/qtest/cases/linux-files.toml` uses a custom initfs:

* `/init` is a controller that starts `initfsd`, waits for `initfs:`, starts `ramfsd`, waits for
  `ramfs:`, starts `vfsd`, and waits for `file:/tmp`;
* `/etc/mounts` mounts `/` on `initfs:` and `/tmp` on `ramfs:`;
* `/data.txt` is a small initfs file;
* `/hello` is a static musl probe that opens `/data.txt`, creates `/tmp/b23.txt`, writes, seeks,
  reads, preads, pwrites, calls `fcntl` and `dup3`, closes, reopens and rereads it.

The probe talks to `vfsd` through `file:` exactly like a normal Raven userland program; linuxd only
translates the Linux path and buffers.

## 4. Evidence

**OBSERVED** from `build/qtest/linux-files.log`:

```
vfsd: mounted / on initfs:
vfsd: mounted /tmp on ramfs:
linux-files: file namespace ready
linux-files: start
linux-files: read initfs ok bytes=9
linux-files: write/read ok
linux-files: pread/pwrite ok
linux-files: fcntl ok
linux-files: dup3 ok
linux-files: reopen ok
linux-files: done
linuxd: syscall close calls=4 errors=0
linuxd: syscall dup3 calls=1 errors=0
linuxd: syscall fcntl calls=4 errors=0
linuxd: syscall lseek calls=4 errors=0
linuxd: syscall openat calls=3 errors=0
linuxd: syscall pread64 calls=1 errors=0
linuxd: syscall pwrite64 calls=1 errors=0
linuxd: syscall read calls=4 errors=0
linuxd: hello exit=0
linux-files: linuxd exit=0
init exited (0)
```

## 5. Limits

* `getdents64` on a remote directory is not translated yet; the B30 synthetic directories keep
  their in-memory listing. The file batch test uses regular files.
* `fcntl` serves descriptor/status duplication and flags; record locks, leases and ownership
  commands are `EINVAL`.
* `F_SETFL` updates the Linux-side flags. The scheme server's append behaviour is fixed at open;
  changing it after open is not forwarded.
* `ufs:` paths use the same `file:` proxy; the B23 qtest exercises `file:` backed by `initfs:` and
  `ramfs:`. A block-backed UFS mount is covered by the fsd-ufs qtests, not this case.
* Multiple Linux processes still cannot be attributed by the trap packet; B23 keeps the existing
  single-process server contract.

## 6. Provenance

First-party BSD-3-Clause code. The syscall numbers and open flags come from the Linux UAPI tables
in `raven-linux-abi`; the `file:`/S11 protocol comes from `raven-proto-fs`/`raven-scheme`. No donor
code was copied.
