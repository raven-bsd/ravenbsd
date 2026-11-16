# libc native spawn (P9a)

SPDX-License-Identifier: BSD-3-Clause
Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

Status: backend implemented; target acceptance is blocked on P9b (`PROC_HANDLE_INFO`).

## Contract

`posix_spawn`/`posix_spawnp` run in the calling process. They read a `file:` image, load it with
the shared `raven-spawn` loader, start the first thread, map the returned process handle to a pid,
and record the pair. `waitpid` looks the pid up, calls `PROC_WAIT`, encodes the exit code as the
Linux wait status (`code << 8`), closes the handle and forgets the entry.

No `fork`, pipes, job control or dynamic linking are added. The child still gets the existing
`INITFS=1` READ|MAP|EXEC rights from `PROC_CREATE`; no ambient rights are granted.

## Pieces

| Piece | Location |
|---|---|
| Loader extraction | `crates/raven-spawn` (moved from `userland/rt/src/spawn.rs`) |
| Runtime API | `userland/rt/src/spawn.rs` re-exports `raven_spawn` |
| Spawner policy and pid table | `libc/ravenlibc/crates/raven-libc-proc/src/native_spawn.rs` |
| Backend wiring | `libc/ravenlibc/crates/raven-libc/src/ffi/procbackend.rs` |
| On-target case | `tests/qtest/cases/libc-spawn.toml` with the C73 C fixtures |

`raven-spawn` closes the `THREAD_CREATE` handle after a successful start; the process handle is
returned to the caller.

## P9b split

| Primitive | Raw | State |
|---|---|---|
| `PROC_HANDLE_INFO(handle) -> pid` | `0x3E` | In P9b's pushed branch, not on `origin/master` |
| `PROC_EXEC_CRED(proc, file, flags)` | `0x3F` | P9b moves U3b's call from `0x3E` to `0x3F` |

`raven-spawn::sys::pid_for_handle` is the only caller of `0x3E`; `RtHandleInfo` wraps it. When P9b
is merged, the kernel supplies the syscall and no P9a code changes are needed. Until then the
on-target `libc-spawn` case fails at the pid conversion and is reported as PARTIAL.

`PROC_WAIT` returns the kernel's raw exit code; `native_spawn` encodes it with
`wait_status::encode(WaitStatus::Exited(code))`, so `WIFEXITED`/`WEXITSTATUS` work.

## Limits in this slice

- Only UTF-8 paths, arguments and environment strings are accepted; other bytes return `EINVAL`.
- `waitpid` supports `P_PID` with blocking options. `wait(2)` (`P_ALL`) and `WNOHANG` return
  `ENOSYS` until the kernel has a wait-any/non-blocking primitive.
- The spawn request's `fdmap` and spawn attributes are not applied yet; the C73 `libc-child`
  fixture opens the console itself. File-action and job-control tickets own those.
- Relative image paths are resolved against `SpawnRequest.cwd`; scheme paths pass unchanged, and
  bare absolute paths become `file:/...`.

## Acceptance

```
cargo nextest run -p raven-spawn
cargo nextest run -p raven-libc-proc
cargo check -p raven-kernel --target x86_64-unknown-none
cargo check -p raven-libc --target x86_64-unknown-none
cargo check -p raven-libc --target x86_64-unknown-ravenbsd
cargo xtask qtest --case libc-spawn --arch x86_64
cargo xtask qtest --case release-login --arch x86_64
make check-fast
```
