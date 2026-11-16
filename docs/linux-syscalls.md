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
---

# Linux syscall coverage: clone (B21b)

Status: B21b, 2026-11-16. Evidence tags: **OBSERVED** (run on the target), **PROPOSED** (not
implemented), **UNVERIFIED** (not exercised).

## 1. Scope

B21b implements `clone` in `linuxd` from the B21a trap frame. The supporting process/thread calls
needed by the static fixture are served too: `gettid`, `set_tid_address`, `set_robust_list`,
`prctl` (`PR_SET_NAME`/`PR_GET_NAME`) and the existing `arch_prctl`.

`clone` covers two shapes:

| shape | flags | native mechanism | child return |
|---|---|---|---|
| thread | `CLONE_VM\|CLONE_THREAD\|CLONE_SIGHAND` | `THREAD_CREATE_EX` in the same process | `rax = 0`, new rsp |
| fork-like | signal byte only (e.g. `SIGCHLD`) | `PROC_FORK` + `THREAD_CREATE_EX` | `rax = 0`, COW stack |

`CLONE_SETTLS`, `CLONE_PARENT_SETTID`, `CLONE_CHILD_SETTID` and `CLONE_CHILD_CLEARTID` are
honoured. Unknown flags, `CLONE_THREAD` without `CLONE_VM`/`CLONE_SIGHAND`, sharing flags without
`CLONE_VM`, a zero/misaligned thread stack, and a fork-like `CLONE_CHILD_SETTID` are `EINVAL`; a
valid `clone` trap without a B21a frame is `ENOSYS`.

## 2. Child frame and trampoline

The kernel delivers the saved user frame in the scheme packet's path field (B21a). linuxd copies it,
sets `gprs[RAX] = 0` and chooses `rsp`:

* thread: the caller's `child_stack` (already 16-byte aligned);
* fork-like: the captured parent `rsp` (the COW copy contains the same stack), unless the caller
  supplied an explicit stack.

linuxd then builds one executable page containing a RIP-relative trampoline and the frame table.
The code loads all 15 general registers and `rsp` from the page, then jumps through a
`jmp qword ptr [rip+disp32]` to the captured RIP. It never touches the stack, so the eight bytes
below the child's `rsp` (the SysV red zone) survive; the `ret` in the fixture's raw-clone stub sees
exactly the parent's stack.

The child TLS base is installed at thread creation (`THREAD_CREATE_EX.tp`); the child does not need
an `arch_prctl` trap.

## 3. Addresses and the signal page

`linuxd` maps one shared, zero-filled page at `0x0000_4800_0000_0000` before the Linux thread
starts. A clone child cannot be attributed to a process by the trap packet (the packet carries no
sender identity), so the fixture children signal the parent through this page and spin instead of
making a syscall. A fork-like child inherits the shared mapping through `PROC_FORK`; a thread child
shares it directly.

Each clone maps a fresh trampoline page at `0x0000_4800_0000_1000 + n * 4096`, after creating the
memory object and populating it through a shared write mapping in linuxd. The target mapping is
created read/write and changed to read/execute with `PROTECT`, because generic memory objects carry
no `EXEC` right in ABI v1.

## 4. Ownership and cleanup

`LinuxProcess` records every native task in `clones` with its Linux pid/tid and native thread id.
When the parent exits (`exit_group`), linuxd terminates and reaps every fork-like child process,
closes the thread handles, and then terminates the Linux process. A thread child needs no separate
termination: terminating the process kills all of its threads.

`CLONE_CHILD_CLEARTID` is passed to the kernel as the new thread's clear-on-exit word. The main
thread's `set_tid_address` word is cleared by the trap loop at `exit_group`, because the thread was
created before the program called `set_tid_address` and the kernel has no later setter.

## 5. Evidence

**OBSERVED** from `build/qtest/linux-clone.log`:

```
linux-clone: gettid tid=6
linux-clone: set_tid_address=6 same=1
linux-clone: set_robust_list ok=1
linux-clone: prctl name=clone-fixture ok=1
linux-clone: arch_prctl fs=4280664 ok=1
linux-clone: clone bad-flags errno=22
linux-clone: clone thread pid=2 child=0
linux-clone: clone thread flag ok
linux-clone: clone fork pid=3 child=0
linux-clone: clone fork flag ok
linux-clone: done
linuxd: hello exit=0
init exited (0)
```

The thread child and the fork child both resumed at the parent's return RIP with `rax = 0` and
wrote the shared page. The fork child was terminated and reaped when the parent exited.

## 6. Limits

* **Trap attribution**: the scheme packet identifies the handler, not the sending process. A clone
  child that made a syscall would be served as its parent. B21b's children signal through the
  shared page and do not trap; per-task attribution is the B21 follow-up.
* `clone` with `CLONE_VM` but without `CLONE_THREAD` (a shared-VM process), namespaces, `CLONE_VFORK`,
  `CLONE_PIDFD` and the other unsupported bits are `EINVAL`, not silently accepted.
* `wait4` is not part of B21b; a fork-like child is reaped by linuxd at parent exit.
* The task-name attributes beyond `PR_SET_NAME`/`PR_GET_NAME` are `EINVAL`; B21 owns the full
  process/thread batch.
* aarch64 keeps the compact trap without a frame; `clone` there is `ENOSYS` until S56 adds frame
  parity.

## 7. Provenance

First-party BSD-3-Clause code. No donor code was copied. The flag values and `ThreadCreateEx`
layout come from `raven-linux-abi` and `raven-abi`; the frame and trampoline are this project's
protocol and user-space code.
