<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# Same-pid exec (`execve`)

P2k adds image replacement in the calling process: `raven_rt::exec::execve` reads an image through
the shared `raven-spawn` file reader, `exec_image` accepts an in-memory image, and the kernel
performs the replacement through the kernel-owned raw call `PROC_EXEC` (`0x6a`). The ABI contract
crate is unchanged.

## Request

`PROC_EXEC(request)` takes one user pointer to this `#[repr(C)]` request (88 bytes, all fields
`u64`):

| Field | Meaning |
|---|---|
| `image`, `image_len` | ELF bytes in the caller's current address space |
| `path`, `path_len` | path spelling for `AT_EXECFN` |
| `argv`, `argc` | user pointer array of NUL-terminated string pointers |
| `envp`, `envc` | environment pointer array |
| `close_list`, `close_count` | handle values to close before the new image starts |
| `flags` | reserved, must be zero |

The runtime packs the argument strings into one owned block and builds the pointer arrays; the
kernel copies the request, the image, the path, every string and the close list through the
fault-safe user-copy layer.

## Replacement sequence

1. Copy and validate the request (`EINVAL`, `E2BIG`, `EFBIG`, `EFAULT`).
2. Parse the ELF with the shared `raven-elf` validator, plan the segments with the same
   `plan_load` used by `/init`, and reject `PT_INTERP`/`ET_DYN` images with `ENOTSUP`.
3. Build a **fresh** `UserAddressSpace`: load the segments, build the C8 argv/envp/auxv stack
   (256 KiB, `ARG_MAX` 128 KiB) with the process's current uid/euid/gid/egid in the auxiliary
   vector.
4. Only after every fallible step succeeded: swap the process's address space, activate the new
   root, update the current thread's cached root, free the old frames/page tables, close the
   explicit and `O_CLOEXEC` handles, clear pending signals, and publish the new entry/stack.
5. The syscall-return path installs the entry/stack on the saved user frame and clears the
   general registers. On success the call never returns.

On any error the temporary address space is dropped, the old image and registers stay intact,
and the error is returned. The qtest proves both an ELF-parser failure and a kernel-side
oversize rejection leave the old process running.

## Identity and handles

The process id, parent, process group, session, credentials, namespace and handle table are
untouched. Non-CLOEXEC handles survive. The explicit `close_list` covers the libc fd-table
CLOEXEC set; pipe ends created with `O_CLOEXEC` are discovered from their endpoint flag and closed
automatically. The qtest verifies a surviving pipe and both close paths.

`AT_UID`/`AT_EUID`/`AT_GID`/`AT_EGID` come from the preserved credentials; `AT_SECURE` is zero.
Applying executable setuid/setgid bits on exec is a follow-up (the spawn path uses
`PROC_EXEC_CRED` for a new child).

## Limits

* Single-threaded callers only (`EBUSY` otherwise). The scheduler has no per-process thread list
  to suspend siblings before the old address space is freed; terminating them needs a scheduler
  ticket.
* The image is copied into kernel memory, so the temporary buffer is capped at 1 MiB (`EFBIG`);
  release user programs are about 200 KiB. Mapping directly from the caller's memory object is a
  follow-up.
* Static `ET_EXEC` images only. `PT_INTERP` and `ET_DYN` return `ENOTSUP`; dynamic linking is a
  later loader ticket.
* `Process.path` keeps the original `/init` spelling; `AT_EXECFN` and argv carry the new path.
* In-flight IPC client/server requests and capture slots are not reset. The caller must be
  single-threaded and not inside another scheme request; a server that execs needs an IPC-state
  reset ticket.
* The exposed client is `userland/rt`; the kernel `PROC_EXEC`/`execve` path is the contract. There is no userspace process server.

## Tests

```
cargo xtask qtest --case proc-exec --arch x86_64
cargo xtask qtest --case proc-exec --arch aarch64
```

The fixture creates a plain pipe, an `O_CLOEXEC` pipe and a pipe named in the explicit close
list, records its identity in the environment, runs a bad image and an oversized raw request
(both must leave it alive), then replaces itself with `/exec-child`. The child verifies argv,
the environment, pid/pgid/sid/credentials, the surviving pipe, and `EBADF` on both closed pipes.
Host tests for the request packing are in `userland/rt/src/exec_ut.rs`.
