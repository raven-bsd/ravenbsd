# libc native spawn (P9a)

SPDX-License-Identifier: BSD-3-Clause
Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

Status: backend complete; the `libc-spawn` target case passes on `origin/master` with P9b merged.

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
| `PROC_HANDLE_INFO(handle) -> pid` | `0x3E` | On master (P9b) |
| `PROC_EXEC_CRED(proc, file, flags)` | `0x3F` | On master (P9b moved U3b's call from `0x3E`) |

`raven-spawn::sys::pid_for_handle` is the only caller of `0x3E`; `RtHandleInfo` wraps it. P9b
supplies the kernel syscall, and the on-target `libc-spawn` case passes on master.

`PROC_WAIT` returns the kernel's raw exit code; `native_spawn` encodes it with
`wait_status::encode(WaitStatus::Exited(code))`, so `WIFEXITED`/`WEXITSTATUS` work.

## P9c follow-up

The Bytes loader is not the fault in the brush repro. A C caller with non-NULL
`posix_spawnattr_t`/`posix_spawn_file_actions_t` and `posix_spawnp` starts the child and waits
correctly (`libc-spawn-attr`). The spawner's file-action model previously saw an empty parent
table, so std-style `adddup2` actions returned `EBADF`; `posix_spawn`/`posix_spawnp` now pass a
snapshot of the platform fds.

Two items remain outside this slice:

* P9c found the `Command::status` sret corruption was a `sigset_t` ABI mismatch: the target
  libc declared an 8-byte `sigset_t` while `sigemptyset`/`sigaddset` and the runtime use 128
  bytes, so std's `MaybeUninit<libc::sigset_t>` on `Command::spawn`'s stack was overrun and
  saved `RBX` was zeroed. `0003-posix-spawn-ravenbsd.patch` now declares the 128-byte
  `s! { struct sigset_t { __val: [c_ulong; 16] } }`; `libc-spawn-std` passes.
* `posix_spawn` transfers each inherited parent handle with `HANDLE_TRANSFER`, encodes the
  `RFDM` block (fd/handle, unknown kind, read/write status), maps it below the child stack and
  publishes `AT_RAVEN_FDMAP`/`AT_RAVEN_FDMAP_N`. Decoding the block in the child belongs to
  `raven-libc/src/start.rs`, which is outside P9c's Owns; the fdmap is published but not yet
  restored, and the spawn tests in this slice do not depend on fd inheritance.

`brush-spawn` remains blocked: the tokio I/O driver wakes through an `eventfd` at
`vendor/tokio/src/runtime/io/driver.rs:279` and gets `Os { code: 9, "Bad file descriptor" }`.
That needs the eventfd `O_RDWR`/read-write routing in
`raven-libc/src/personality_common.rs` and `unistd/{read,write}.rs`, the binary-safe `bytes_n`
fix in `ffi/cstr.rs`/`unistd/{write,pwrite}.rs`, the start-path `restore_fdmap`, and the brush
`echo` pre-sizing. All of those files are outside P9c's Owns, so this branch keeps only the
loader/spawner changes and reports the blocker instead of carrying out-of-scope edits.

## Limits in this slice

- Only UTF-8 paths, arguments and environment strings are accepted; other bytes return `EINVAL`.
- `waitpid` uses `PROC_WAIT_EX`: `P_PID`, `P_ALL`, `P_PGID` and `WNOHANG` work; stop/continue
  events follow the kernel encoding.
- The spawn request's `fdmap` is encoded for static ET_EXEC images but the child does not
  restore it yet (`start.rs` is out of scope). Spawn attributes and an interpreter plus fdmap
  combination are not implemented; the C73 `libc-child` fixture opens the console itself, and
  job-control tickets own the rest.
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
cargo xtask qtest --case libc-spawn-std --arch x86_64
cargo xtask qtest --case brush-spawn --arch x86_64
cargo xtask qtest --case release-login --arch x86_64
make check-fast
```
