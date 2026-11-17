<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# Same-pid exec (`execve`)

P2k adds image replacement in the calling process: `raven_rt::exec::execve` reads an image through
the shared `raven-spawn` file reader, `exec_image` accepts an in-memory image, and the kernel
performs the replacement through the kernel-owned raw call `PROC_EXEC` (`0x6a`). The ABI contract
crate is unchanged.

S81 extends the same path to dynamic userland: an `ET_DYN` main with `PT_INTERP` maps the
interpreter from the boot initfs, builds the dynamic auxiliary vector and transfers control to
the interpreter entry. This is the normal launch path for a dynamically linked executable; no
explicit `ld-elf.so.1 <program>` invocation is involved.

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
2. Parse the ELF with the shared `raven-elf` validator. `ET_EXEC` maps at its link addresses;
   `ET_DYN` maps at `MAIN_PIE_BASE` (`0x0000_1000_0000_0000`, the same base `raven-spawn` uses).
   If the image declares `PT_INTERP`, read the interpreter from the boot initfs, parse it as
   `ET_DYN`, map it at `INTERP_BASE` (`0x0000_2000_0000_0000`), and reject a missing, nested or
   malformed interpreter with `ENOENT`/`ENOEXEC`. Plan both images with `plan_load_biased`.
3. Build a **fresh** `UserAddressSpace`: load the interpreter (if any) and the main segments,
   then build the C8 argv/envp/auxv stack (256 KiB, `ARG_MAX` 128 KiB) with the process's current
   uid/euid/gid/egid in the auxiliary vector.
4. Only after every fallible step succeeded: swap the process's address space, activate the new
   root, update the current thread's cached root, free the old frames/page tables, close the
   explicit and `O_CLOEXEC` handles, clear pending signals, and publish the new entry/stack.
5. The syscall-return path installs the entry/stack on the saved user frame and clears the
   general registers. On success the call never returns.

On any error the temporary address space is dropped, the old image and registers stay intact,
and the error is returned. The qtest proves both an ELF-parser failure and a kernel-side
oversize rejection leave the old process running.

## Dynamic image contract (S81)

The kernel owns the load biases and the initial auxiliary vector; the loader owns the
`DT_NEEDED` closure and relocations. The two halves meet on these exact values:

| Value | Static `ET_EXEC` | `ET_DYN` (no `PT_INTERP`) | `ET_DYN` + `PT_INTERP` |
|---|---|---|---|
| main load bias | 0 | `MAIN_PIE_BASE` | `MAIN_PIE_BASE` |
| interpreter load bias | — | — | `INTERP_BASE` |
| `AT_PHDR` | main PHDR | main PHDR + main bias | main PHDR + main bias |
| `AT_PHENT` | 56 | 56 | 56 |
| `AT_PHNUM` | main `e_phnum` | main `e_phnum` | main `e_phnum` |
| `AT_BASE` | 0 | 0 | `INTERP_BASE` |
| `AT_ENTRY` | main `e_entry` | main `e_entry` + main bias | main `e_entry` + main bias |
| initial PC | main entry | main entry | interpreter `e_entry` + `INTERP_BASE` |
| `AT_EXECFN` | request path | request path | request path |

`MAIN_PIE_BASE` and `INTERP_BASE` are the same constants `raven-spawn` uses, so an exec'd
process and a spawned process have the same image layout. `AT_PHDR` is required for a dynamic
main: an image whose program-header table is not covered by a `PT_LOAD`/`PT_PHDR` is rejected
with `ENOEXEC`. The interpreter path must be absolute; the kernel looks it up in the boot initfs
only (there is no userspace file server call in this path).

The interpreter entry contract (C74g's `ld-elf.so.1`) is: enter `_rtld_start` with the C8 stack
pointer in `rsp`, parse `argc`/`argv`/`envp`/auxv, use `AT_PHDR`/`AT_PHNUM` for the
already-mapped main (do not re-map it), use `AT_BASE` as the loader's own load bias, resolve the
`DT_NEEDED` closure through the file scheme, and transfer to `AT_ENTRY` on the original stack.
The kernel does not map or resolve `libc.so.1`; that remains the loader's job.

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
* The image is copied into kernel memory, so the temporary main-image buffer is capped at 1 MiB
  (`EFBIG`); release user programs are about 200 KiB. The interpreter is read from the initfs and
  capped at 2 MiB. Mapping directly from the caller's memory object is a follow-up.
* Dynamic main images are `ET_DYN`. A static-PIE `ET_DYN` without `PT_INTERP` is also accepted.
  A dynamic main needs a `PT_INTERP` image present in the boot initfs; a missing interpreter is
  `ENOENT`, a non-`ET_DYN`/nested interpreter is `ENOEXEC`, and an oversized interpreter is
  `EFBIG`. The kernel does not load `DT_NEEDED` libraries; the interpreter does.
* `Process.path` keeps the original `/init` spelling; `AT_EXECFN` and argv carry the new path.
* In-flight IPC client/server requests and capture slots are not reset. The caller must be
  single-threaded and not inside another scheme request; a server that execs needs an IPC-state
  reset ticket.
* The exposed client is `userland/rt`; the kernel `PROC_EXEC`/`execve` path is the contract. There is no userspace process server.

## Tests

```
cargo xtask qtest --case proc-exec --arch x86_64
cargo xtask qtest --case proc-exec --arch aarch64
cargo xtask qtest --case dynamic-exec --arch x86_64
```

`dynamic-exec` builds a normally linked PIE with `PT_INTERP` and `DT_NEEDED libc.so.1`, embeds it
in a static launcher, and runs the launcher as a `libc-test` service. The launcher replaces itself
through `PROC_EXEC`; the expected serial is `dynamic-exec: launching`, `svcd: settled`,
`init exited (0)`. The fixture's shape check fails if the target is not `ET_DYN`, has no
`PT_INTERP /libexec/ld-elf.so.1`, or does not need `libc.so.1`.

The fixture creates a plain pipe, an `O_CLOEXEC` pipe and a pipe named in the explicit close
list, records its identity in the environment, runs a bad image and an oversized raw request
(both must leave it alive), then replaces itself with `/exec-child`. The child verifies argv,
the environment, pid/pgid/sid/credentials, the surviving pipe, and `EBADF` on both closed pipes.
Host tests for the request packing are in `userland/rt/src/exec_ut.rs`.
