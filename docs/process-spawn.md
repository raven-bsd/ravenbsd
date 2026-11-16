<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# Native spawn for std programs (P9)

Status: spike/design. P9 changes no product code and claims no board or QEMU result. The
follow-up tickets in this document are the implementation; the existing `libc-spawn` case is
still red at this base.

## Verdict

The smallest safe path is an **in-process libc spawn** that reads an ELF image from the `file:`
scheme and drives the existing `PROC_CREATE` / `MAP` / `THREAD_CREATE` sequence. The loader
already exists in `userland/rt/src/spawn.rs`; P10 extracts it into a shared `no_std` crate and
both `raven-rt` and `ravenlibc` call it. No new server, no procd RPC, and no fd transfer are
required for the 0.1.1 target:

```
brush -c '/bin/ls /bin'
brush -c '/sbin/ping -c 1 10.0.2.2'
```

Two small additions are prerequisites:

* `PROC_HANDLE_INFO` (raw `0x3E`), because `PROC_WAIT` returns `0` and `PROC_PID_INFO` takes a
  pid, so nothing can turn the `PROC_CREATE` handle into the `pid_t` that `posix_spawn` must
  return.
* A libc `pid -> process handle` table plus `PROC_WAIT` status encoding, so `waitpid` can reap
  the child and return `WEXITSTATUS` to Rust std.

Interactive brush is still blocked by the K26b input/TLS work. P9 removes the external-command
blocker only; it does not by itself make brush the interactive login shell.

## Current blocker (OBSERVED)

Rust std on RavenBSD reaches `posix_spawnp`, which builds a `SpawnRequest` through
`raven-libc-proc` and asks the process backend for a child. On the target:

* `libc/ravenlibc/crates/raven-libc/src/ffi/procbackend.rs` returns
  `SyscallBackend<RealSyscall>`; `SyscallBackend` owns `LocalBackend::new(None)`.
* `LocalBackend::spawn` returns `ENOSYS` when `early` is `None`; `wait` returns `ECHILD`.
* `libc/ravenlibc/crates/raven-libc/src/process/spawn_env.rs` returns `ENOSYS` for `open` and
  `is_dir`; `SpawnCtx.table` is always `&[]`.
* `libc/ravenlibc/crates/raven-libc/src/process/posix_spawn.rs` never supplies an early
  spawner and never records the returned process handle.

`/bin/rsh` works because it is a `raven-rt` program: it calls `raven_rt::spawn::spawn_with`
directly with a `GrantSys`. A std program has no such call path. This is the P9 blocker.

The ticket text says "a std shell also has no initfs grant for the raven-rt loader". That
premise is stale at this base: `kernel/src/process/mod.rs::new_empty_process` now calls
`handle::install_initial(..., false)`, so every `PROC_CREATE` child receives
`SELF_PROC=0`, `INITFS=1` (`READ|MAP|EXEC`) and `ROOT_NAMESPACE=2`. The first-level spawn needs
no grant transfer at all. What a child still lacks is `TRANSFER` on the initfs, which is needed
only to make the child a spawner in turn.

## Existing pieces to reuse

| Piece | Location | What it provides |
|---|---|---|
| Loader | `userland/rt/src/spawn.rs` | `spawn_with` / `spawn_image_with`, ELF planning, stack image, `Sys` seam, `GrantSys` |
| File-image read | `userland/rt/src/spawn.rs::read_file_image` | bounded 8 MiB `file:` read |
| Spawn model | `libc/ravenlibc/crates/raven-libc-proc/src/spawn_model/` | `SpawnRequest`, file actions, attrs, `posix_spawnp` PATH search |
| Early seam | `raven-libc-proc/src/backend.rs::EarlySpawn` | the shell-side spawn hook already modelled by `LocalBackend` |
| Child fdmap | `raven-libc-platform/src/fd/inherit.rs` | `FdMap` encode/decode, `restore_fdmap`; not wired in `start.rs` yet |
| Child stdio | `docs/spawn-stdio.md` (K06c) | each child opens `tty:console` at 0/1/2 in libc start |
| Caller credentials | `tickets/stuart/reports/U3a.md` | `file:` requests carry uid/euid/gid/egid and the permission matrix |

U3a is not merged at the P9 base. The design assumes it lands before file permissions become a
security boundary; the release image binaries are world-readable; the loader itself performs no
permission check and trusts the `file:` server.

One loader bug is in scope for the extraction: `spawn_elf` receives the `THREAD_CREATE` handle
and ignores it, so every spawn leaves one thread handle in the parent's table. P10 closes it
with `HANDLE_CLOSE`; the host fixture asserts that call.

## Grant model

### Handles in a fresh process

`install_initial` inserts, in this order:

| Handle | Object | Rights | Scope |
|---|---|---|---|
| `0` | `SELF_PROC` | `ALL` | every process |
| `1` | `INITFS` | `READ\|MAP\|EXEC` | every process |
| `2` | `ROOT_NAMESPACE` | `READ` | every process |
| `3` | `ROOT_DEVICE` | `ADMIN\|DUP\|TRANSFER` | first process only (`initfs_exec=true`) |

The initfs is an immutable archive of the system image. `READ|MAP|EXEC` lets a process read and
map a system binary; it does not let it write the archive or reach a device. This is the
existing kernel policy (`e7ad675b`, "grant READ|MAP|EXEC on the initfs to every process") and
P9 does not widen it.

### The missing rights transfer

Two distinctions matter:

1. **First-level spawn needs no transfer.** The parent opens/reads the image from `file:` or maps
   `INITFS` into the child with its own `READ|MAP` handle. The child receives mappings, not the
   source handle. `PROC_CREATE` returns the child handle with `Rights::ALL`, so `MAP`, `PROTECT`,
   `UNMAP` and `THREAD_CREATE` have the rights they need.
2. **Nested spawn needs `TRANSFER`.** A child that must itself spawn needs an initfs handle with
   `TRANSFER`. `install_initial(false)` deliberately withholds that bit. The sanctioned path is a
   service grant: `etc/services.d/<svc>.toml` sets `grants = ["initfs"]`;
   `GrantSpec::Initfs` requests `READ|MAP|EXEC|TRANSFER`; `GrantSys::thread_create` calls
   `HANDLE_TRANSFER(child, INITFS, mask)` before `RtSys::thread_create`. Because the child
   already holds 0-2, the transferred handle lands at **handle 3**. Its number must be published
   to the child (a new `AT_RAVEN_INITFS` auxv tag, or a fixed well-known slot), and the loader
   must use it instead of `Handle::INITFS`. Do **not** add `TRANSFER` to every process.
3. **fd handoff needs per-handle transfer.** Scheme handles from `OPEN` carry `TRANSFER`
   (`kernel/src/ipc/router.rs`). Passing a descriptor to a child means
   `HANDLE_TRANSFER(child, handle, rights)` for every distinct handle, then writing the
   child-valid values into the `RFDM` fdmap block. This is P15 and is out of the console-only
   path.

"No ambient rights for arbitrary std programs" therefore means: a spawned std program gets only
the three well-known handles, never a device, `ROOT_DEVICE`, or `TRANSFER`. The new ability is
the use of the already-ambient initfs read/map rights through libc, not a new kernel capability.

## Chosen API path

```
std::process::Command
  -> posix_spawnp
     -> spawn_model::spawn_model_pid          (existing; validates and applies actions)
        -> EarlySpawn::spawn(path, argv, envp, cwd, fdmap)
           -> raven-spawn::spawn_image_with   (extracted from userland/rt)
              -> PROC_CREATE / MAP / PROTECT / THREAD_CREATE
```

`posix_spawn` and `posix_spawnp` keep the existing model. Only the target backend changes: a new
`EarlySpawn` implementation in `raven-libc` replaces the `LocalBackend` `ENOSYS` for `spawn` and
provides the pid/process-handle table. The seam needs a matching wait/reap operation (or
`SyscallBackend` must hold the same spawner for `wait`), because `LocalBackend::wait` currently
returns `ECHILD` and has no table to consult. Host builds keep `LocalBackend` unchanged.

### Image source: `file:`

The early spawner reads the image with the libc fd layer (`open`, `read`, `close`) from
`file:<path>`, bounded by `MAX_FILE_IMAGE` (8 MiB), then calls `spawn_image_with`. Reasons:

* It needs no initfs archive parser in libc and no `raven-initfs` dependency.
* It works unchanged for a UFS root (`file:/root/bin/ls`) and for the initfs root.
* It uses the ordinary `file:` permission path, so U3a's caller credentials apply.
* One image copy per exec, bounded at 8 MiB, is acceptable for 0.1.1.

Limitations to state in the implementation:

* The read requires `R`; POSIX exec requires only `X`. A future `file:` exec-open operation (or
  `Call::Access(X)` plus a privileged read) is needed for execute-only binaries. All release
  binaries are `0755`, so the 0.1.1 gate is unaffected.
* `posix_spawnp`'s `ENOEXEC` fallback to `/bin/sh` becomes reachable only after `/bin/sh` itself
  can be spawned; keep it soft until then.
* Relative paths are resolved against `SpawnRequest.cwd` before the `file:` open.

### Alternatives

| Option | Cost | Verdict |
|---|---|---|
| In-process file-image spawn (chosen) | shared loader + libc backend + `PROC_HANDLE_INFO` | smallest change; no new service; reuses the proven loader |
| In-process initfs zero-copy spawn | shared loader + `raven-initfs` in libc + archive mapping | faster, but larger dependency surface; follow-up optimization |
| procd RPC | new server, wire protocol, handle return, fdmap transfer | correct long-term shape, but far larger; deferred |

procd remains the right owner once sessions, job control, accounting and setuid exec arrive.
For 0.1.1 it adds a service that must itself be spawned, a request protocol that must return a
process handle to the caller, and an fdmap handoff that the console-only path does not need.

## Exact syscall sequence

The sequence below is the one in `userland/rt/src/spawn.rs`, with the P9 additions marked. All
lengths are page-aligned; `MAP` addresses are inside `USER_MIN..USER_END`.

### Image and address space

1. `file:` read of the image, at most `MAX_FILE_IMAGE` bytes.
2. `raven_elf::plan_load` with `LoadMode::Strict` for `ET_EXEC` and `LoadMode::Copy` for
   `ET_DYN`; the `ET_DYN` bias is `MAIN_PIE_BASE` (`0x0000_1000_0000_0000`), the interpreter bias
   is `INTERP_BASE` (`0x0000_2000_0000_0000`). `PT_INTERP` is planned but is out of scope for
   0.1.1.
3. `MEMOBJ_CREATE(page_len, 0)` (`0x43`) -> image object. The object has
   `READ|WRITE|MAP|DUP|TRANSFER`, **not** `EXEC`.
4. `MAP(SELF_PROC, image_obj, 0, 0, page_len, READ|WRITE|SHARED)` (`0x40`, flags
   `0x1|0x2|(0x2<<8)`) -> parent scratch. Copy the image bytes; `UNMAP(SELF_PROC, scratch,
   page_len)` (`0x41`).
5. `PROC_CREATE(0)` (`0x30`) -> child handle (`Rights::ALL`, native personality).
6. For each planned segment: `MAP(child, image_obj, file_offset, vaddr, len, prot)`.
   For a segment with `EXEC`, map it `READ|WRITE` first, then
   `PROTECT(child, vaddr, len, READ|EXEC)` (`0x42`) before any thread exists. The kernel rejects
   `WRITE|EXEC` together and a generic memory object has no `EXEC` right, so this two-step is
   mandatory.
7. `StepKind::Zero` (bss): `MAP(child, Handle::NONE, 0, vaddr, len, prot)` (`0x40` with a zero
   object handle; anonymous zero fill).
8. Partial file tail: create a one-page object, map it in the parent, map the source page in the
   parent, copy the tail, map the page into the child at `vaddr`, `PROTECT` to the segment
   protection, close the temporary object. This is the `copy_tail` path in the loader.

### Stack and first thread

9. `MEMOBJ_CREATE(STACK_SIZE, 0)` (`0x43`), `STACK_SIZE = 256 KiB` (at least `ARG_MAX`).
10. `MAP(SELF_PROC, stack_obj, 0, 0, STACK_SIZE, READ|WRITE|SHARED)`, write the C8 stack
    (`argv`, `envp`, auxv, `AT_RANDOM`, `AT_PLATFORM`), `UNMAP(SELF_PROC, scratch, STACK_SIZE)`.
11. `MAP(child, stack_obj, 0, STACK_TOP - STACK_SIZE, STACK_SIZE,
    READ|WRITE|FIXED|SHARED)` (`STACK_TOP = USER_END`), then `HANDLE_CLOSE(stack_obj)`.
12. `THREAD_CREATE(child, entry, sp0, sp0, 0)` (`0x20`). Flags are zero: the main thread's TLS
    is installed by libc start from `AT_PHDR` / `AT_PHNUM` / `PT_TLS`; `THREAD_CREATE_EX` is for
    `pthread_create`, not for the first thread.
13. Close the temporary image object(s) and the thread handle (if returned). The parent keeps the
    child process handle for `PROC_WAIT` and closes it after the reap.

### libc wrapper: pid and wait

14. `PROC_HANDLE_INFO(child, out, 8)` (`0x3E`, **new**) -> `[pid, ppid, pgid, sid, uid, euid,
    gid, egid]`; the wrapper records `pid -> child` for `waitpid`. This call lives in the libc
    `EarlySpawn` wrapper after `spawn_image_with` returns, not in the shared loader, so the
    loader's `(Handle)` return type and its other callers do not change. It must succeed for a
    live handle even when the child has already exited, or a fast child races the lookup. On
    failure, `PROC_TERMINATE(child, code)` (`0x32`, permitted by `Rights::ALL`), `PROC_WAIT`,
    then close.
15. `PROC_WAIT(child, &code)` (`0x31`) blocks until exit and writes the raw exit code. The libc
    backend encodes it as POSIX wait status `(code & 0xff) << 8`, returns the recorded pid, and
    removes the table entry, so a second `waitpid` returns `ECHILD`.
16. `HANDLE_CLOSE(child)` (`0x50`).

`fork`, `exec`, `pipe2` and signals are not part of this sequence.

## `posix_spawn` mapping

| POSIX input | Target behaviour | Status |
|---|---|---|
| `path` / `argv` / `envp` | `SpawnRequest.path` / `argv` / `envp`; stack image | required |
| `posix_spawnp` PATH search | existing `search_path`, `/bin/sh` fallback soft | required |
| `pid` out | `PROC_HANDLE_INFO` on the returned child handle; table insert | required (new syscall) |
| `waitpid` | table lookup, `PROC_WAIT`, status encode, `ECHILD` after reap | required |
| `POSIX_SPAWN_SETSIGMASK` / `SETSIGDEF` | accepted by the attr object; no child mask syscall yet | no-op, documented |
| `POSIX_SPAWN_SETPGROUP` | `PROC_SETPGID(pid, pgroup)` (`0x36`) before `THREAD_CREATE`; needs the pid earlier than the wrapper has it | out of scope; in-loader pid or handle-based setpgid |
| `POSIX_SPAWN_SETSID` | no child-setsid syscall (`PROC_SETSID` is self only) | out of scope |
| `POSIX_SPAWN_SETSCHED*` | no scheduler syscall | out of scope |
| `umask` | `SpawnRequest` field; no child side | not applied |
| `CLOEXEC_DEFAULT` | fdmap only | out of scope |
| `Open` / `Dup2` / `Fchdir` / `TcSetPgrp` file actions | need the parent fd snapshot and handle transfer | out of scope (P15) |
| `Close` / `CloseFrom` | applied to the scratch table; a child has no inherited fds | harmless no-op |
| `Chdir` / `addchdir_np` | `SpawnRequest.cwd`; emit `AT_RAVEN_CWD` (`0x7005`) and apply it in start | required for P14 |

The cwd needs explicit wiring. `raven-libc-platform` has `Cwd::from_pwd`/`Platform::init_cwd` and
`startup.rs` decodes `AT_RAVEN_CWD`, but `Platform::new` starts at `/` and `start.rs` does not
call either. A login shell that `cd`s and then runs an external command needs the loader to place
the absolute cwd string in the stack image and emit `AT_RAVEN_CWD`, and `start.rs` to call
`set_cwd` from it before `main`. P10 emits the key; P11 applies it. `PWD` alone is not a
fallback: nothing consumes it at process start today.

`build_auxv` currently writes `AT_UID/EUID/GID/EGID = 0`. The kernel child inherits the parent's
`ProcessIds` (`ProcessIds::inherited`), so `getuid()` is right, but `getauxval(AT_UID)` lies for
a non-root parent. Replace the four zeros with the parent's `PROC_SELF_INFO` values (or drop the
keys and let libc use the syscall) before multi-user login.

## Explicitly out of scope for 0.1.1

* `pipe2` / pipes, `posix_spawn` file-action redirection, `popen`, `system` with a pipe, and
  process substitution (PG-35 and P15).
* Job control: controlling terminal, `tcsetpgrp`, process-group-aware waits, `SIGTSTP`/`SIGCONT`.
* `fork` and `exec` (PG-45/46). `posix_spawn` is the only creation path.
* Signals in the child: signal mask, default dispositions, signal delivery.
* Dynamic linking and `PT_INTERP` beyond the existing loader support; the 0.1.1 tools are static.
* Setuid/setgid exec, `POSIX_SPAWN_RESETIDS`, and credential changes in the child.
* Nested spawn: a spawned child cannot spawn its own child until the initfs `TRANSFER` grant and
  handle-number publication land (P16).
* procd sessions, accounting, `/proc`, `WNOHANG` and `waitid`.
* aarch64 or board validation. The design is architecture-neutral; only QEMU x86_64 evidence is
  planned for the follow-up.
* Interactive brush input (K26b's static TLS, basic-backend and reedline blockers).

## Test plan

### `tests/spawn/` host fixture

The fixture is a planning artifact, not a runnable product test in P9. It defines the transcript
the extracted loader must produce so the host mock cannot silently drift:

* `tests/spawn/README.md` — what the fixture is for, and which ticket activates it.
* `tests/spawn/expected-sequence.txt` — the ordered `Sys` calls and exact arguments for the
  two-segment image case: `MEMOBJ_CREATE`, parent `MAP`/`UNMAP`, `PROC_CREATE`, child `MAP` per
  segment, `PROTECT` for text, stack `MEMOBJ_CREATE`/`MAP`/`UNMAP`/`MAP(child)`, `HANDLE_CLOSE`,
  `THREAD_CREATE`, then the libc wrapper's `PROC_HANDLE_INFO`, `PROC_WAIT`, `HANDLE_CLOSE`.
  It must also assert the negative: no `HANDLE_TRANSFER`, no `PROC_SETPGID`, no `fork`/`exec`.
* `tests/spawn/cases/brush-ls.toml` and `tests/spawn/cases/brush-ping.toml` — the target qtest
  sketches below; the follow-up moves them to `tests/qtest/cases/`.

The existing `userland/rt/src/spawn_ut.rs` already covers the loader's plan and error paths with
a recording mock. P10 moves that coverage to `crates/raven-spawn` and keeps this fixture as the
libc-facing contract.

### Target qtests

1. **`libc-spawn` (existing, currently red).** This is the strongest gate. It must print all of
   `spawn ok`, `wait ok`, `wait twice ok`, `enoent ok`, `spawnp ok`, `exit codes ok`,
   `getpid ok`, `sigdefault ok`, `fork enosys ok`, `done`. It covers `pid > 0`, `waitpid`,
   `ECHILD`, exit codes 0/1/255, `posix_spawnp` PATH search and the `fork` `ENOSYS` contract.
2. **`spawn-brush` (new).** Boot the `release` profile, log in as root (the 0.1.0 `rsh` login is
   fine), then send `/bin/brush -c '/bin/ls /bin'` and require the `cat`, `ls`, `uname` lines
   and a clean exit. This proves std `Command` -> `posix_spawnp` -> loader without changing the
   login shell.
3. **`spawn-ping` (new).** Same image plus the QEMU user-net devices from `release-login`, then
   `/bin/brush -c '/sbin/ping -c 1 10.0.2.2'`; require the `ping:` reply and summary lines and no
   timeout. This proves a second, non-libc binary and the `net:icmp` scheme from a spawned
   child.
4. **Regressions.** `release-login`, `login-session`, `shell-session`, `base-tools`,
   `libc-spawn` and `make check-fast`. The first must keep logging in with `/bin/rsh` and
   pinging 10.0.2.2 until P14 flips the account.

No command above is executed by P9. The follow-up reports the exit codes and captured guest
lines; P9 claims no target result.

## Ticket split

| Ticket | Size | Owner | Scope | Depends |
|---|---|---|---|---|
| P10 | M | otto | Extract `spawn_image_with`/`spawn_elf`/stack/`Sys` from `userland/rt/src/spawn.rs` into `crates/raven-spawn`; `raven-rt` re-exports it; move `spawn_ut.rs`; add `AT_RAVEN_CWD` and identity-aware auxv | — |
| P11 | L | dave | Target `EarlySpawn` in libc: `file:` image read, `PROC_CREATE`/`MAP`/`THREAD_CREATE`, `pid -> handle` table, `waitpid` status, `ShellSpawnEnv::open`, `SpawnCtx.table` snapshot point, apply `AT_RAVEN_CWD` in `start.rs` | P10, P12 |
| P12 | S | stuart | Kernel `PROC_HANDLE_INFO` (raw `0x3E`): 8-word identity for a live process handle, including an exited-but-unreaped child; `READ` right, host test and ABI constant | — |
| P13 | S | otto | `spawn-brush` and `spawn-ping` qtests from `tests/spawn/cases/`; release image wiring; evidence report | P11 |
| P14 | S | kevin | Flip the 0.1.1 login shell to `/bin/brush` after P13 and K26b; update `etc/passwd`, release profile, `release-login`, `docs/login.md`; keep rsh as fallback | P13, K26b |
| P15 | M | dave | fd inheritance: `HANDLE_TRANSFER` per distinct fd, `RFDM` block in a mapped object, `AT_RAVEN_FDMAP`/`_N`, `restore_fdmap` in `start.rs`, `SpawnCtx.table` snapshot | P11, U3a |
| P16 | M | stuart | Nested spawn: initfs `TRANSFER` grant, transferred handle publication (`AT_RAVEN_INITFS`), loader handle selection | P15 |
| P17 | L | kevin | procd RPC with the same `SpawnRequest` contract; sessions and job control | P11 |

Sizes are implementation estimates, not schedule commitments. P10 and P11 are the only tickets
on the 0.1.1 critical path; P15-P17 are follow-ups.

## Risk to the 0.1.0 login path

P9 itself is docs and test planning only: no product code, no image, no login change, so the
0.1.0 path cannot regress from this ticket.

The follow-ups touch two sensitive layers:

* **libc process code** is linked by every program. `posix_spawn` and `waitpid` change from
  honest `ENOSYS`/`ECHILD` to real dispatch. Keep the host `LocalBackend` unchanged, keep the
  new code behind `cfg(target_os = "none")`, and gate on `libc-spawn`, `shell-session`,
  `base-tools`, `release-login` and `make check-fast`. `fork`/`exec` stay `ENOSYS`.
* **kernel `PROC_HANDLE_INFO`** is additive (new raw number, no change to existing syscalls).
  The `PROC_WAIT` return value stays `0`; only libc encodes the status.

The 0.1.0 release keeps `/bin/rsh` as the login shell. `/bin/brush` stays installed and
`brush -c` becomes usable, but the account flip waits for P14 and the K26b interactive fixes.
Rollback is one line in `etc/passwd` plus the release profile; rsh remains the tested fallback.
Do not merge P14 before `release-login` is green with brush, and do not claim a board result from
the QEMU gate.

## References

* `tickets/otto/todo/P9-native-spawn-spike.md` — contract.
* `userland/rt/src/spawn.rs` — canonical loader sequence.
* `libc/ravenlibc/crates/raven-libc-proc/src/spawn_model/` — model, actions, driver.
* `libc/ravenlibc/crates/raven-libc-platform/src/fd/inherit.rs` — `RFDM` block.
* `docs/spawn-stdio.md` — K06c console adoption.
* `docs/libc-kernel-triage.md` — `libc-spawn` current failures.
* `tickets/stuart/reports/U3a.md` — caller credentials and the permission matrix.
* `tickets/carl/reports/C57b.md` — `THREAD_CREATE_EX`, TLS and the C73c startup gap.
* `docs/login.md` — getty/login shell handoff.
* `crates/raven-abi/src/syscall.rs` — syscall numbers.
* `kernel/src/process/handle.rs` — `install_initial` handle policy.
