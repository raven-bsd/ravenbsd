<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# Exec image loading (K220)

Status: **OBSERVED** for the libc `execve` path on x86_64 QEMU; the 10.3 MiB fixture and the
failure-atomicity case pass in `exec-large`.

## Old path

The libc `execve`/`execv` path ended in `SyscallBackend::exec` -> `LocalBackend::exec`, which
returned `ENOSYS` (`libc/ravenlibc/crates/raven-libc-proc/src/local_backend.rs`). The kernel
`PROC_EXEC` request copied the entire image into a kernel `Vec` and rejected anything above
`MAX_EXEC_IMAGE = 1 MiB` (`kernel/src/syscall/exec.rs`, `kernel/src/process/exec.rs`). The shared
`raven-spawn` reader capped file images at 8 MiB, so the staged OpenSSH helpers
(`sshd-session` 8,398,408 B, `sshd-auth` 8,351,088 B) could not pass either limit.

## New path

1. libc `execve`/`execv` still collect the C strings, then
   `SyscallBackend::exec` calls `raven-libc-proc::exec_raw::exec`.
2. `exec_raw` reads the image through `raven-spawn::read_file_image` (or `read_file_handle` for
   `fexecve`) at the raised **16 MiB** `MAX_FILE_IMAGE`, packs the path/argv/envp/CLOEXEC request
   and issues raw `PROC_EXEC` (`0x6a`). The image buffer is in the caller's user address space.
3. The kernel validates the request, copies only the 64-byte ELF header, the complete
   program-header table and any `PT_INTERP` string into a small kernel buffer (at most 64 KiB),
   and calls `Elf::parse_with_len` with the real image length.
4. `load_segments_from_user` plans the `PT_LOAD` steps and copies each page from the caller's
   still-live image into a fresh frame. The 4 MiB kernel heap never holds the image.
5. The existing `replace_image` tail keeps the pid, ids, credentials, namespace, handle table
   and CLOEXEC semantics, then installs the new entry/stack. Any parse, bounds or copy failure
   drops the temporary address space and returns the documented errno with the old image and
   registers intact.

`MAX_EXEC_IMAGE` in the kernel and `MAX_FILE_IMAGE` in `raven-spawn` are 16 MiB. The
`userland/rt` convenience `exec_image` wrapper keeps its separate smaller self-limit; the libc
path does not use it.

## Failure atomicity

`exec-large` first writes `/tmp/not-an-elf` and calls `execve` on it. The parser rejects it with
`ENOEXEC` while the caller keeps running and prints `exec-large: bad-elf refused errno=8`; only
then does the successful 10.3 MiB exec run.

## Test profile and fixture

- `tests/qtest/cases/exec-large.toml` uses the `libc-test` profile, whose service manifest runs
  `/bin/libc-test` with `scheme:file` and `consd`.
- `tests/fixtures/proc/build.sh exec-large` builds a static libc init and a static child with a
  9 MiB read-only payload (10,772,488 B linked). The init calls libc `execve`; the child prints
  the same pid, `argv[3] = {path, alpha, beta}` and `PROC_EXEC_TOKEN=K220`.
- `tests/fixtures/proc/src/bin/exec.rs` continues to cover the raw oversized-request failure
  against the 16 MiB kernel bound.

## Donor reconciliation

| Topic | Illumos | BSD | Linux | Raven decision |
|---|---|---|---|---|
| exec image loading | exec behaviour only; CDDL, no copy | FreeBSD/NetBSD exec outcomes only | Linux execve outcome only | first-party streaming/segment-copy `PROC_EXEC` from the user image; no donor code. |
| size limits | not applicable | not applicable | not applicable | bound by the address space and a checked 16 MiB input, never a 4 MiB heap copy. |

No donor code was copied.
